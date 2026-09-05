use compact_str::CompactString;
use net_client::EngineSet;
use parser_pipeline::TelemetryRoute;
use payload_gen::input::{
    InputHub, Persona, RawEvent, TabEvents, TabInput, TabSession, TelemetryBatcher, BATCH_CAP,
    BATCH_INTERVAL_US, events_bytes,
};
use session_state::{CookieJar, Profile, Session};
use smallvec::SmallVec;
use std::sync::Arc;

pub struct PushJob {
    pub tab: u32,
    pub slot: usize,
    pub endpoint: CompactString,
    pub transport: parser_pipeline::Transport,
    pub field: CompactString,
    pub blob: Vec<u8>,
    pub events: u64,
}

struct FleetSlot {
    session: Session,
    engine_slot: usize,
    site: u32,
    route: TelemetryRoute,
    batcher: TelemetryBatcher,
    scratch: Vec<RawEvent>,
    events_total: u64,
    batches: u32,
}

pub struct Fleet {
    hub: InputHub,
    slots: Vec<Option<FleetSlot>>,
    free: Vec<u32>,
    engines: Arc<EngineSet>,
}

impl Fleet {
    pub fn new(engines: Arc<EngineSet>) -> Self {
        Self {
            hub: InputHub::new(),
            slots: Vec::new(),
            free: Vec::new(),
            engines,
        }
    }

    pub fn attach(
        &mut self,
        profile: Arc<Profile>,
        origin: &str,
        jar: &CookieJar,
        route: TelemetryRoute,
        engine_slot: usize,
        weight: u32,
    ) -> u32 {
        let site = self.hub.register_site(3);
        let tab = self.hub.open_tab(site, weight);        let persona = Persona::derive(profile.canvas_seed, tab.0 as u64);
        let vw = profile.screen_w.max(320) as f64;
        let vh = profile.screen_h.max(240) as f64;
        let seed = profile.canvas_seed ^ (tab.0 as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let mut rng = payload_gen::input::SplitMix64Rng::new(seed);
        let from = (vw * (0.08 + rng.next_f64() * 0.1), vh * (0.1 + rng.next_f64() * 0.12));
        let target = (vw * (0.45 + rng.next_f64() * 0.25), vh * (0.4 + rng.next_f64() * 0.25));
        let trust = self.trust_of(site);
        let session = TabSession::start(
            persona,
            seed,
            tab.0 as u64,
            from,
            target,
            30.0,
            None,
            now_us(),
            trust,
        );
        let mut http_session = Session::new(profile, origin);
        for (name, value) in jar.iter() {
            http_session.jar.ingest(&format!("{name}={value}"));
        }
        let slot = FleetSlot {
            session: http_session,
            engine_slot,
            site,
            route,
            batcher: TelemetryBatcher::new(BATCH_CAP, BATCH_INTERVAL_US),
            scratch: Vec::with_capacity(BATCH_CAP + 32),
            events_total: 0,
            batches: 0,
        };
        let idx = if let Some(i) = self.free.pop() {
            self.slots[i as usize] = Some(slot);
            i
        } else {
            self.slots.push(Some(slot));
            (self.slots.len() - 1) as u32
        };
        self.hub.set_input(tab, TabInput::Session(session));
        idx
    }

    pub fn calibrate(&self, tab: u32, ok: bool) {
        if let Some(site) = self.tab_site(tab) {
            if let Some(c) = self.hub.calibration(site) {
                c.record(ok);
            }
        }
    }

    fn tab_site(&self, tab: u32) -> Option<u32> {
        self.slots
            .get(tab as usize)
            .and_then(|s| s.as_ref().map(|s| s.site))
    }

    fn trust_of(&self, site: u32) -> i32 {
        self.hub.calibration(site).map(|c| c.trust()).unwrap_or(0)
    }

    pub fn next_due_us(&self) -> u64 {
        self.hub.next_due_us()
    }

    pub fn live_tabs(&self) -> usize {
        self.hub.live_tabs()
    }

    pub fn pump(&mut self, now_us: u64, jobs: &mut SmallVec<[PushJob; 8]>) {
        jobs.clear();
        let mut events: TabEvents = SmallVec::new();
        self.hub.tick(now_us, &mut events);
        for (tab, ev) in events {
            let Some(slot) = self.slots.get_mut(tab.0 as usize).and_then(|s| s.as_mut()) else {
                continue;
            };
            slot.scratch.push(ev);
            slot.events_total += 1;
            slot.batcher.feed(1);
        }
        for i in 0..self.slots.len() {
            let Some(slot) = self.slots[i].as_mut() else { continue };
            if slot.batcher.ready(now_us) && !slot.scratch.is_empty() {
                slot.batches += 1;
                let blob = events_bytes(&slot.scratch).to_vec();
                let n = slot.scratch.len() as u64;
                slot.scratch.clear();
                slot.batcher.begin(now_us);
                jobs.push(PushJob {
                    tab: i as u32,
                    slot: slot.engine_slot,
                    endpoint: slot.route.endpoint.clone(),
                    transport: slot.route.transport,
                    field: slot.route.field.clone(),
                    blob,
                    events: n,
                });
            }
        }
    }

    pub async fn push(&mut self, job: &PushJob) -> u16 {
        let Some(slot) = self.slots.get_mut(job.tab as usize).and_then(|s| s.as_mut()) else {
            return 0;
        };
        let route = TelemetryRoute {
            provider: slot.route.provider,
            endpoint: job.endpoint.clone(),
            transport: job.transport,
            field: job.field.clone(),
        };
        net_client::push_telemetry(&self.engines, job.slot, &mut slot.session, &route, &job.blob)
            .await
            .unwrap_or(0)
    }

    pub fn stats(&self) -> serde_json::Value {
        let total_events: u64 = self
            .slots
            .iter()
            .filter_map(|s| s.as_ref().map(|s| s.events_total))
            .sum();
        let total_batches: u32 = self
            .slots
            .iter()
            .filter_map(|s| s.as_ref().map(|s| s.batches))
            .sum();
        serde_json::json!({
            "liveTabs": self.live_tabs(),
            "events": total_events,
            "batches": total_batches,
        })
    }
}

pub enum FleetMsg {
    Attach {
        profile: Arc<Profile>,
        origin: String,
        cookies: Vec<(String, String)>,
        route: TelemetryRoute,
        engine_slot: usize,
        weight: u32,
        reply: tokio::sync::oneshot::Sender<u32>,
    },
    Stats {
        reply: tokio::sync::oneshot::Sender<serde_json::Value>,
    },
}

pub async fn fleet_daemon(
    mut fleet: Fleet,
    mut rx: tokio::sync::mpsc::Receiver<FleetMsg>,
    stats: crate::stats::StatsRef,
) {
    let mut jobs: SmallVec<[PushJob; 8]> = SmallVec::new();
    loop {
        let next = fleet.next_due_us();
        let now = now_us();
        let wait_us = next.saturating_sub(now).min(250_000).max(1_000);
        match tokio::time::timeout(std::time::Duration::from_micros(wait_us), rx.recv()).await {
            Ok(Some(FleetMsg::Attach { profile, origin, cookies, route, engine_slot, weight, reply })) => {
                let mut jar = CookieJar::new();
                for (k, v) in cookies {
                    jar.ingest(&format!("{k}={v}"));
                }
                let tab = fleet.attach(profile, &origin, &jar, route, engine_slot, weight);
                let _ = reply.send(tab);
            }
            Ok(Some(FleetMsg::Stats { reply })) => {
                let _ = reply.send(fleet.stats());
            }
            Ok(None) => break,
            Err(_) => {}
        }
        let now = now_us();
        fleet.pump(now, &mut jobs);
        for job in &jobs {
            let status = fleet.push(job).await;
            stats.add_touches(job.events);
            fleet.calibrate(job.tab, status / 100 == 2);
        }
    }
}

pub fn now_us() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
