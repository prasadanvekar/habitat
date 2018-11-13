// Copyright (c) 2016 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cmp::{Ordering, PartialOrd};
use std::collections::HashMap;
use std::num::ParseIntError;
use std::result;
use std::str::FromStr;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::thread;

use time::Duration;

use butterfly;
use common::ui::UI;
use env;
use hcore::package::{PackageIdent, PackageInstall, PackageTarget};
use hcore::service::ServiceGroup;
use launcher_client::LauncherCli;

use census::CensusRing;
use config::EnvConfig;
use manager::periodic::Periodic;
use manager::service::{Service, Topology, UpdateStrategy};
use util;

static LOGKEY: &'static str = "SU";
// TODO (CM): Yes, the variable value should be "period" and not
// "frequency"... we need to fix that.
const PERIOD_BYPASS_CHECK_ENVVAR: &'static str = "HAB_UPDATE_STRATEGY_FREQUENCY_BYPASS_CHECK";
lazy_static! {
    static ref MIN_ALLOWED_PERIOD: Duration = Duration::seconds(60);
}

type UpdaterStateList = HashMap<ServiceGroup, UpdaterState>;

enum UpdaterState {
    AtOnce(Receiver<PackageInstall>),
    Rolling(RollingState),
}

enum RollingState {
    AwaitingElection,
    InElection,
    Leader(LeaderState),
    Follower(FollowerState),
}

enum LeaderState {
    Polling(Receiver<PackageInstall>),
    Waiting,
}

/// Current package update state of a follower in a leader-follower
/// topology
enum FollowerState {
    /// Waiting to be told to update
    Waiting,
    /// Currently updating
    Updating(Receiver<PackageInstall>),
}

/// The ServiceUpdater is in charge of updating a Service when a more recent version of a package
/// has been published to a depot or installed to the local package cache.
/// To use an update strategy, the supervisor must be configured to watch a depot for new versions.
pub struct ServiceUpdater {
    states: UpdaterStateList,
    butterfly: butterfly::Server,
}

impl ServiceUpdater {
    pub fn new(butterfly: butterfly::Server) -> Self {
        ServiceUpdater {
            states: UpdaterStateList::default(),
            butterfly: butterfly,
        }
    }

    /// Register a new `Service` for updates. Returns `true` if the
    /// `ServiceUpdater` was modified (i.e., the given service has an
    /// `UpdateStrategy` that is not `None`).
    pub fn add(&mut self, service: &Service) -> bool {
        match service.update_strategy {
            UpdateStrategy::None => false,
            UpdateStrategy::AtOnce => {
                self.states
                    .entry(service.service_group.clone())
                    .or_insert_with(|| {
                        let rx = Worker::new(service).start(&service.service_group, None);
                        UpdaterState::AtOnce(rx)
                    });
                true
            }
            UpdateStrategy::Rolling => {
                self.states
                    .entry(service.service_group.clone())
                    .or_insert(UpdaterState::Rolling(RollingState::AwaitingElection));
                true
            }
        }
    }

    // TODO (CM): How do we remove something from the updater? e.g.,
    // when we stop or unload a service?

    /// See if the given service has an update. Returns `true` if a
    /// new version was installed, thus signalling that the service
    /// should be restarted
    pub fn check_for_updated_package(
        &mut self,
        service: &mut Service,
        census_ring: &CensusRing,
        launcher: &LauncherCli,
    ) -> bool {
        let mut updated = false;
        match self.states.get_mut(&service.service_group) {
            Some(&mut UpdaterState::AtOnce(ref mut rx)) => match rx.try_recv() {
                Ok(package) => {
                    service.update_package(package, launcher);
                    return true;
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    debug!("Service Updater worker has died; restarting...");
                    *rx = Worker::new(service).start(&service.service_group, None);
                }
            },

            Some(&mut UpdaterState::Rolling(ref mut st @ RollingState::AwaitingElection)) => {
                if let Some(census_group) = census_ring.census_group_for(&service.service_group) {
                    if service.topology == Topology::Leader {
                        debug!(
                            "Rolling Update, determining proper suitability because we're in \
                             a leader topology"
                        );
                        match (census_group.me(), census_group.leader()) {
                            (Some(me), Some(leader)) => {
                                let suitability = if me.member_id == leader.member_id {
                                    u64::min_value()
                                } else {
                                    u64::max_value()
                                };
                                self.butterfly.start_update_election(
                                    &service.service_group,
                                    suitability,
                                    0,
                                );
                                *st = RollingState::InElection
                            }
                            _ => return false,
                        }
                    } else {
                        debug!("Rolling update, using default suitability");
                        self.butterfly
                            .start_update_election(&service.service_group, 0, 0);
                        *st = RollingState::InElection;
                    }
                }
            }
            Some(&mut UpdaterState::Rolling(ref mut st @ RollingState::InElection)) => {
                if let Some(census_group) = census_ring.census_group_for(&service.service_group) {
                    match (census_group.me(), census_group.update_leader()) {
                        (Some(me), Some(leader)) => {
                            if me.member_id == leader.member_id {
                                debug!("We're the leader");
                                // Start in waiting state to ensure all members agree with our
                                // version before attempting a new rolling upgrade.
                                *st = RollingState::Leader(LeaderState::Waiting);
                            } else {
                                debug!("We're a follower");
                                *st = RollingState::Follower(FollowerState::Waiting);
                            }
                        }
                        (Some(_), None) => return false,
                        _ => return false,
                    }
                }
            }
            Some(&mut UpdaterState::Rolling(RollingState::Leader(ref mut state))) => {
                match *state {
                    LeaderState::Polling(ref mut rx) => match rx.try_recv() {
                        Ok(package) => {
                            debug!("Rolling Update, polling found a new package");
                            service.update_package(package, launcher);
                            updated = true;
                        }
                        Err(TryRecvError::Empty) => return false,
                        Err(TryRecvError::Disconnected) => {
                            debug!("Service Updater worker has died; restarting...");
                            *rx = Worker::new(service).start(&service.service_group, None);
                        }
                    },
                    LeaderState::Waiting => {
                        match census_ring.census_group_for(&service.service_group) {
                            Some(census_group) => {
                                if census_group.members().iter().any(|cm| {
                                    cm.pkg.as_ref().unwrap()
                                        != census_group.me().unwrap().pkg.as_ref().unwrap()
                                }) {
                                    debug!("Update leader still waiting for followers...");
                                    return false;
                                }
                                let rx = Worker::new(service).start(&service.service_group, None);
                                *state = LeaderState::Polling(rx);
                            }
                            None => panic!(
                                "Expected census list to have service group '{}'!",
                                &*service.service_group
                            ),
                        }
                    }
                }
                if updated {
                    *state = LeaderState::Waiting;
                }
            }
            Some(&mut UpdaterState::Rolling(RollingState::Follower(ref mut state))) => {
                match *state {
                    FollowerState::Waiting => {
                        match census_ring.census_group_for(&service.service_group) {
                            Some(census_group) => match (
                                census_group.update_leader(),
                                census_group.previous_peer(),
                                census_group.me(),
                            ) {
                                (Some(leader), Some(peer), Some(me)) => {
                                    if leader.pkg == me.pkg {
                                        debug!("We're not in an update");
                                        return false;
                                    }
                                    if leader.pkg != peer.pkg {
                                        debug!("We're in an update but it's not our turn");
                                        return false;
                                    }
                                    debug!("We're in an update and it's our turn");
                                    let rx = Worker::new(service)
                                        .start(&service.service_group, leader.pkg.clone());
                                    *state = FollowerState::Updating(rx);
                                }
                                _ => return false,
                            },
                            None => panic!(
                                "Expected census list to have service group '{}'!",
                                &*service.service_group
                            ),
                        }
                    }
                    FollowerState::Updating(ref mut rx) => match census_ring
                        .census_group_for(&service.service_group)
                    {
                        Some(census_group) => match rx.try_recv() {
                            Ok(package) => {
                                service.update_package(package, launcher);
                                updated = true
                            }
                            Err(TryRecvError::Empty) => return false,
                            Err(TryRecvError::Disconnected) => {
                                debug!("Service Updater worker has died; restarting...");
                                let package = census_group.update_leader().unwrap().pkg.clone();
                                *rx = Worker::new(service).start(&service.service_group, package);
                            }
                        },
                        None => panic!(
                            "Expected census list to have service group '{}'!",
                            &*service.service_group
                        ),
                    },
                }
                if updated {
                    *state = FollowerState::Waiting;
                }
            }
            None => {}
        }
        updated
    }
}

/// Represents how far apart checks for updates to individual services
/// are, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceUpdatePeriod(Duration);

impl Default for ServiceUpdatePeriod {
    fn default() -> Self {
        ServiceUpdatePeriod(*MIN_ALLOWED_PERIOD)
    }
}

impl FromStr for ServiceUpdatePeriod {
    type Err = ParseIntError;
    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        // Parsing as a u32 gives us an effective range of 49+ days
        let raw = s.parse::<u32>()?;
        Ok(Duration::milliseconds(raw as i64).into())
    }
}

impl From<Duration> for ServiceUpdatePeriod {
    fn from(d: Duration) -> Self {
        ServiceUpdatePeriod(d)
    }
}

impl Into<Duration> for ServiceUpdatePeriod {
    fn into(self) -> Duration {
        self.0
    }
}

impl PartialOrd<Duration> for ServiceUpdatePeriod {
    fn partial_cmp(&self, other: &Duration) -> Option<Ordering> {
        Some(self.0.cmp(other))
    }
}

impl PartialEq<Duration> for ServiceUpdatePeriod {
    fn eq(&self, other: &Duration) -> bool {
        self.0 == *other
    }
}

impl EnvConfig for ServiceUpdatePeriod {
    // TODO (CM): Yes, the variable value should be "period" and not
    // "frequency"... we need to fix that.
    const ENVVAR: &'static str = "HAB_UPDATE_STRATEGY_FREQUENCY_MS";
}

struct Worker {
    current: PackageIdent,
    spec_ident: PackageIdent,
    builder_url: String,
    channel: String,
}

impl Periodic for Worker {
    // TODO (CM): Consider performing this check once and storing it,
    // instead of re-checking every time.
    fn update_period(&self) -> Duration {
        let val = ServiceUpdatePeriod::configured_value().into();
        if val < *MIN_ALLOWED_PERIOD {
            if env::var(PERIOD_BYPASS_CHECK_ENVVAR).is_ok() {
                val
            } else {
                *MIN_ALLOWED_PERIOD
            }
        } else {
            val
        }
    }
}

impl Worker {
    fn new(service: &Service) -> Self {
        Worker {
            current: service.pkg.ident.clone(),
            spec_ident: service.spec_ident.clone(),
            builder_url: service.bldr_url.clone(),
            channel: service.channel.clone(),
        }
    }

    /// Start a new update worker.
    ///
    /// Passing an optional package identifier will make the worker perform a run-once update to
    /// retrieve a specific version from Builder. If no package identifier is specified,
    /// then the updater will poll until a newer more suitable package is found.
    fn start(mut self, sg: &ServiceGroup, ident: Option<PackageIdent>) -> Receiver<PackageInstall> {
        let (tx, rx) = sync_channel(0);
        thread::Builder::new()
            .name(format!("service-updater-{}", sg))
            .spawn(move || match ident {
                Some(latest) => self.run_once(tx, latest),
                None => self.run_poll(tx),
            }).expect("unable to start service-updater thread");
        rx
    }

    // TODO (CM): A refactor I'd like to do is to tease out the
    // run_once and run_poll cases into two separate "start" functions
    // that describe more what's going on. Passing `None` as the
    // identifier just means to keep going until you get a new
    // one. Passing an identifier (which should probably be a
    // fully-qualified one, right?) just goes until that package gets
    // downloaded.
    //
    // In all cases except for FollowerState::Updating and
    // FollowerState::Waiting, we pass None, so that's easy. In those
    // two states, though, the package can legitimately be an Option
    // (we get it from CensusMember.pkg), but it seems like it can
    // only be None if there was an unparseable identifier in the
    // ServiceRumor the CensusMember was generated from. I suspect
    // that we might be able to refactor the types (or how we handle
    // them) a bit better.
    //
    // I'm also not 100% clear why we have run_poll and run_once,
    // since their implementations are very similar. There may be an
    // opportunity to collapse those.
    /// Polls until a newer version of the specified package is
    /// available. When such a package is found, it is installed, and
    /// the function exits.
    fn run_once(&mut self, sender: SyncSender<PackageInstall>, ident: PackageIdent) {
        // Fairly certain that this only gets called in a rolling update
        // scenario, where `ident` is always a fully-qualified identifier
        outputln!("Updating from {} to {}", self.current, ident);
        let install_source = (ident, *PackageTarget::active_target()).into();
        loop {
            let next_time = self.next_period_start();

            match util::pkg::install(
                // We don't want anything in here to print
                &mut UI::with_sinks(),
                &self.builder_url,
                &install_source,
                &self.channel,
            ) {
                Ok(package) => {
                    self.current = package.ident().clone();
                    sender.send(package).expect("Main thread has gone away!");
                    break;
                }
                Err(e) => warn!("Failed to install updated package: {:?}", e),
            }

            self.sleep_until(next_time);
        }
    }

    /// Continually poll for a new version of a package, installing it
    /// when found.
    fn run_poll(&mut self, sender: SyncSender<PackageInstall>) {
        let install_source = (self.spec_ident.clone(), *PackageTarget::active_target()).into();
        loop {
            let next_time = self.next_period_start();

            match util::pkg::install(
                // We don't want anything in here to print
                &mut UI::with_sinks(),
                &self.builder_url,
                &install_source,
                &self.channel,
            ) {
                Ok(maybe_newer_package) => {
                    if self.current < *maybe_newer_package.ident() {
                        outputln!(
                            "Updating from {} to {}",
                            self.current,
                            maybe_newer_package.ident()
                        );
                        self.current = maybe_newer_package.ident().clone();
                        sender
                            .send(maybe_newer_package)
                            .expect("Main thread has gone away!");
                        break;
                    } else {
                        debug!("Package found is not newer than ours");
                    }
                }
                Err(e) => warn!("Updater failed to get latest package: {:?}", e),
            }

            self.sleep_until(next_time);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_update_period_is_equal_to_minimum_allowed_value() {
        assert_eq!(ServiceUpdatePeriod::default().0, *MIN_ALLOWED_PERIOD);
    }

    locked_env_var!(HAB_UPDATE_STRATEGY_FREQUENCY_MS, lock_period_var);
    locked_env_var!(HAB_UPDATE_STRATEGY_FREQUENCY_BYPASS_CHECK, lock_bypass_var);

    fn worker() -> Worker {
        Worker {
            current: "core/testing/1.0.0/20181109125930"
                .parse()
                .expect("Can't parse ident!"),
            spec_ident: "core/testing".parse().expect("Can't parse ident!"),
            builder_url: String::from("https://bldr.habitat.sh"),
            channel: String::from("stable"),
        }
    }

    #[test]
    fn service_update_period_must_be_positive() {
        assert!(ServiceUpdatePeriod::from_str("-123").is_err());
        assert!(ServiceUpdatePeriod::from_str("0").is_ok());
        assert!(ServiceUpdatePeriod::from_str("5").is_ok());
    }

    #[test]
    fn worker_period_defaults_properly() {
        let period = lock_period_var();
        let bypass = lock_bypass_var();
        let worker = worker();

        period.unset();
        bypass.unset();

        assert_eq!(ServiceUpdatePeriod::default(), worker.update_period());
    }

    #[test]
    fn worker_period_can_be_overridden_by_env_var() {
        let period = lock_period_var();
        let bypass = lock_bypass_var();
        let worker = worker();

        period.set("120000");
        bypass.unset();
        let expected_period: ServiceUpdatePeriod = Duration::milliseconds(120000).into();
        assert!(expected_period >= *MIN_ALLOWED_PERIOD);
        assert_eq!(expected_period, worker.update_period());
    }

    #[test]
    fn worker_period_cannot_be_overridden_to_a_very_small_value_by_default() {
        let period = lock_period_var();
        let bypass = lock_bypass_var();
        let worker = worker();

        period.set("1"); // This is TOO low
        bypass.unset();
        assert!(Duration::milliseconds(1) < *MIN_ALLOWED_PERIOD);
        assert_eq!(ServiceUpdatePeriod::default(), worker.update_period());
    }

    #[test]
    fn worker_period_cannot_be_overridden_by_a_non_number() {
        let period = lock_period_var();
        let bypass = lock_bypass_var();
        let worker = worker();

        period.set("this is not a number");
        bypass.unset();
        assert_eq!(ServiceUpdatePeriod::default(), worker.update_period());
    }

    #[test]
    fn worker_period_can_be_overridden_by_a_small_value_with_bypass_var() {
        let period = lock_period_var();
        let bypass = lock_bypass_var();
        let worker = worker();

        period.set("5000");
        bypass.set("1");
        let expected_period: ServiceUpdatePeriod = Duration::milliseconds(5000).into();
        assert!(expected_period < *MIN_ALLOWED_PERIOD);
        assert_eq!(expected_period, worker.update_period());
    }
}
