use super::{error, Manager};
use cruciblebox_next2_protocol::{self as protocol, RpcError};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(PartialEq)]
struct Topic {
    provider: String,
    version: String,
    capability: String,
    event: String,
}

struct Subscription {
    owner: String,
    session: String,
    topic: Topic,
    expires: Instant,
    pending: VecDeque<(u64, Arc<Value>)>,
    sequence: u64,
    dropped: u64,
}

#[derive(Default)]
pub(super) struct Events(Mutex<HashMap<String, Subscription>>);

impl Events {
    fn subscribe(&self, owner: &str, session: &str, topic: Topic) -> Result<Value, String> {
        let now = Instant::now();
        let mut subscriptions = self.0.lock().map_err(|_| "EVENTS_UNAVAILABLE")?;
        subscriptions.retain(|_, item| now < item.expires);
        if subscriptions.len() >= protocol::BUDGET_EVENT_SUBSCRIPTIONS
            || subscriptions
                .values()
                .filter(|item| item.session == session)
                .count()
                >= protocol::BUDGET_EVENT_SUBSCRIPTIONS_PER_SESSION
        {
            return Err("EVENT_SUBSCRIPTION_LIMIT".into());
        }
        let provider = topic.provider.clone();
        let id = crate::rand_token::random_token_hex()?;
        subscriptions.insert(
            id.clone(),
            Subscription {
                owner: owner.into(),
                session: session.into(),
                topic,
                expires: now + Duration::from_millis(protocol::BUDGET_LEASE_MS as u64),
                pending: VecDeque::new(),
                sequence: 0,
                dropped: 0,
            },
        );
        Ok(json!({"subscriptionId":id,"providerId":provider}))
    }

    fn publish(&self, topic: Topic, payload: Value) -> Result<Value, String> {
        let now = Instant::now();
        let mut subscriptions = self.0.lock().map_err(|_| "EVENTS_UNAVAILABLE")?;
        subscriptions.retain(|_, item| now < item.expires);
        let payload = Arc::new(payload);
        let mut delivered = 0;
        for subscription in subscriptions
            .values_mut()
            .filter(|item| item.topic == topic)
        {
            if subscription.pending.len() == protocol::BUDGET_EVENT_QUEUE {
                subscription.pending.pop_front();
                subscription.dropped += 1;
            }
            subscription.sequence += 1;
            subscription
                .pending
                .push_back((subscription.sequence, payload.clone()));
            delivered += 1;
        }
        Ok(json!({"delivered":delivered}))
    }

    fn poll(&self, session: &str, id: &str) -> Result<Value, String> {
        let now = Instant::now();
        let mut subscriptions = self.0.lock().map_err(|_| "EVENTS_UNAVAILABLE")?;
        let subscription = subscriptions
            .get_mut(id)
            .filter(|item| item.session == session && now < item.expires)
            .ok_or("EVENT_SUBSCRIPTION_UNAVAILABLE")?;
        subscription.expires = now + Duration::from_millis(protocol::BUDGET_LEASE_MS as u64);
        let event = subscription.pending.pop_front().map(|(sequence, payload)| json!({
            "sequence":sequence,"publisherId":subscription.topic.provider,
            "capabilityId":subscription.topic.capability,"event":subscription.topic.event,"payload":&*payload,
        }));
        let dropped = std::mem::take(&mut subscription.dropped);
        Ok(json!({"event":event,"dropped":dropped}))
    }

    fn unsubscribe(&self, session: &str, id: &str) -> Result<Value, String> {
        let mut subscriptions = self.0.lock().map_err(|_| "EVENTS_UNAVAILABLE")?;
        if subscriptions
            .get(id)
            .is_some_and(|item| item.session == session)
        {
            subscriptions.remove(id);
            Ok(json!(true))
        } else {
            Ok(json!(false))
        }
    }

    pub(super) fn clear_session(&self, session: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "EVENTS_UNAVAILABLE")?
            .retain(|_, item| item.session != session);
        Ok(())
    }

    pub(super) fn clear_plugin(&self, owner: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "EVENTS_UNAVAILABLE")?
            .retain(|_, item| item.owner != owner && item.topic.provider != owner);
        Ok(())
    }
}

impl Manager {
    pub(super) fn event_service(
        &self,
        row: &crate::db::PluginRow,
        request: &protocol::Request,
    ) -> Result<Value, RpcError> {
        let params = &request.params;
        let name = || params["event"].as_str().unwrap();
        let capability = || params["capabilityId"].as_str().unwrap();
        let subscription_id = || params["subscriptionId"].as_str().unwrap();
        match request.method.as_str() {
            "events.subscribe" => {
                let registry = self.registry().map_err(error)?;
                let binding = registry
                    .capability(&row.name, capability())
                    .map_err(error)?;
                protocol::capability_event(&binding.export, name()).map_err(error)?;
                self.events.subscribe(
                    &row.name,
                    &request.session,
                    Topic {
                        provider: binding.provider_id.clone(),
                        version: binding.provider_version.clone(),
                        capability: capability().into(),
                        event: name().into(),
                    },
                )
            }
            "events.publish" => {
                let db = self
                    .db
                    .lock()
                    .map_err(|_| error("STORAGE_UNAVAILABLE".into()))?
                    .clone();
                let manifest =
                    crate::next2_install::load_manifest(&db, Path::new(&row.installed_path))
                        .map_err(error)?;
                Self::verify_record(&manifest, &row.name, &row.version, &row.permissions)
                    .map_err(error)?;
                let export = manifest
                    .exports
                    .iter()
                    .find(|item| item.id == capability())
                    .ok_or_else(|| error("EVENT_UNDECLARED".into()))?;
                let descriptor = protocol::capability_event(export, name()).map_err(error)?;
                protocol::validate_event_payload(descriptor, &params["payload"]).map_err(error)?;
                self.events.publish(
                    Topic {
                        provider: row.name.clone(),
                        version: row.version.clone(),
                        capability: capability().into(),
                        event: name().into(),
                    },
                    params["payload"].clone(),
                )
            }
            "events.poll" => self.events.poll(&request.session, subscription_id()),
            "events.unsubscribe" => self.events.unsubscribe(&request.session, subscription_id()),
            _ => Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
        }
        .map_err(error)
    }
}
