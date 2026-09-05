use crate::catalog::CatalogEntry;
use std::sync::Arc;
use std::time::Duration;
use wreq::IntoEmulation as _;
use wreq::redirect::Policy;
use wreq_util::Emulation;

pub struct EngineSet {
    clients: Vec<Arc<wreq::Client>>,
}

impl EngineSet {
    pub fn build(catalog: &[CatalogEntry]) -> Result<Self, wreq::Error> {
        let mut clients = Vec::with_capacity(catalog.len());
        for entry in catalog {
            let emulation = Emulation::builder()
                .profile(entry.wprofile)
                .platform(entry.wplatform)
                .build()
                .into_emulation();
            let client = wreq::Client::builder()
                .emulation(emulation)
                .redirect(Policy::limited(5))
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(256)
                .pool_max_size(1024)
                .tcp_nodelay(true)
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(30))
                .build()?;
            clients.push(Arc::new(client));
        }
        Ok(Self { clients })
    }

    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    pub fn client_for(&self, slot: usize) -> &Arc<wreq::Client> {
        &self.clients[slot % self.clients.len()]
    }
}
