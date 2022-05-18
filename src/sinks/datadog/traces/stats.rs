use std::collections::BTreeMap;

use crate::{
    event::{TraceEvent, Value},
    metrics::AgentDDSketch,
    sinks::datadog::traces::sink::PartitionKey,
};
mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

const TOP_LEVEL_KEY: &str = "_dd.top_level";

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct AggregationKey {
    payload_key: PayloadAggregationKey,
    bucket_key: BucketAggregationKey,
}

impl AggregationKey {
    fn NewAggregationFromSpan(span: &BTreeMap<String, Value>, origin: String, payload_key: PayloadAggregationKey) -> Self {
        AggregationKey {
            payload_key: payload_key.clone(),
            bucket_key: BucketAggregationKey {
                service: span
                    .get("service")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                name: span
                    .get("name")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                resource: span
                    .get("resource")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                ty: span
                    .get("type")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                status_code: 0,
            },
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PayloadAggregationKey {
    env: String,
    hostname: String,
    version: String,
    container_id: String,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct BucketAggregationKey {
    service: String,
    name: String,
    resource: String,
    ty: String,
    status_code: u32,
}

struct GroupedStats {
    hits: f64,
    top_level_hits: f64,
    errors: f64,
    duration: f64,
    ok_distribution: AgentDDSketch,
    err_distribution: AgentDDSketch,
}

impl GroupedStats {
    fn export(&self, key: &AggregationKey) -> dd_proto::ClientGroupedStats {
        dd_proto::ClientGroupedStats {
            service: key.bucket_key.service.clone(),
            name: key.bucket_key.name.clone(),
            resource: key.bucket_key.resource.clone(),
            http_status_code: key.bucket_key.status_code,
            r#type: key.bucket_key.ty.clone(),
            db_type: "".to_string(),
            hits: self.hits.round() as u64,
            errors: self.errors.round() as u64,
            duration: self.duration.round() as u64,
            ok_summary: vec![], // TODO !!! convert
            error_summary: vec![],
            synthetics: false,
            top_level_hits: self.top_level_hits.round() as u64,
        }
    }
}

struct Bucket {
    start: u64,    // timestamp of start in our format
    duration: u64, // duration of a bucket in nanoseconds
    data: BTreeMap<AggregationKey, GroupedStats>,
}

impl Bucket {
    fn export(&self) -> BTreeMap<PayloadAggregationKey, dd_proto::ClientStatsBucket> {
        let mut m = BTreeMap::<PayloadAggregationKey, dd_proto::ClientStatsBucket>::new();
        self.data.iter().for_each(|(k, v)| {
            let b = v.export(k);
            match m.get_mut(&k.payload_key) {
                None => {
                    let sb = dd_proto::ClientStatsBucket {
                        start: self.start,
                        duration: self.duration,
                        agent_time_shift: 0,
                        stats: vec![b],
                    };
                    m.insert(k.payload_key.clone(), sb);
                },
                Some(s) => {
                    s.stats.push(b);
                },
            };
        });
        m
    }
}

struct Aggregator {

}

impl Aggregator {
    fn new() -> Self {
        Self {}
    }

    fn handle_trace(&self, _trace: &TraceEvent) {
        //trace.get
    }

    fn handle_span(&self, span: &BTreeMap<String, Value>, weight: f64, origin: String, aggKey: PayloadAggregationKey) {

    }

    fn get_client_stats_payload(&self) -> Vec<dd_proto::ClientStatsPayload> {
        vec![]
    }
}

fn has_top_level(span: &BTreeMap<String, Value>) -> bool {
    span.get("metrics")
        .and_then(|m| m.as_object())
        .map(|m| match m.get(TOP_LEVEL_KEY) {
            Some(Value::Float(f)) => f.into_inner().signum() == 1.0,
            _ => false,
        })
        .unwrap_or(false)
}

pub(crate) fn compute_apm_stats(
    key: &PartitionKey,
    traces: &[TraceEvent],
) -> dd_proto::StatsPayload {
    let aggregator = Aggregator::new();
    traces.iter().for_each(|t| aggregator.handle_trace(t));
    dd_proto::StatsPayload {
        agent_hostname: key.hostname.clone().unwrap_or_else(|| "".to_string()),
        agent_env: key.env.clone().unwrap_or_else(|| "".to_string()),
        stats: aggregator.get_client_stats_payload(),
        agent_version: key.agent_version.clone().unwrap_or_else(|| "".to_string()),
        client_computed: false,
    }
}
