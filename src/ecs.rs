//! `aws ecs list-clusters` / `describe-clusters` / `list-services` /
//! `describe-services` shell-outs + structured response models.
//! Pure CLI — no SDK dep.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    #[serde(rename = "clusterName")]
    pub name: String,
    #[serde(rename = "clusterArn", default)]
    pub arn: String,
    #[serde(rename = "status", default)]
    pub status: Option<String>,
    #[serde(rename = "registeredContainerInstancesCount", default)]
    pub container_instances: Option<u32>,
    #[serde(rename = "runningTasksCount", default)]
    pub running_tasks: Option<u32>,
    #[serde(rename = "pendingTasksCount", default)]
    pub pending_tasks: Option<u32>,
    #[serde(rename = "activeServicesCount", default)]
    pub active_services: Option<u32>,
    #[serde(rename = "capacityProviders", default)]
    pub capacity_providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    #[serde(rename = "id", default)]
    pub id: String,
    #[serde(rename = "status", default)]
    pub status: Option<String>,
    #[serde(rename = "taskDefinition", default)]
    pub task_definition: Option<String>,
    #[serde(rename = "desiredCount", default)]
    pub desired_count: Option<u32>,
    #[serde(rename = "runningCount", default)]
    pub running_count: Option<u32>,
    #[serde(rename = "pendingCount", default)]
    pub pending_count: Option<u32>,
    #[serde(rename = "rolloutState", default)]
    pub rollout_state: Option<String>,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEvent {
    #[serde(rename = "id", default)]
    pub id: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    #[serde(rename = "message", default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    #[serde(rename = "serviceName")]
    pub name: String,
    #[serde(rename = "serviceArn", default)]
    pub arn: String,
    #[serde(rename = "clusterArn", default)]
    pub cluster_arn: String,
    #[serde(rename = "status", default)]
    pub status: Option<String>,
    #[serde(rename = "desiredCount", default)]
    pub desired_count: Option<u32>,
    #[serde(rename = "runningCount", default)]
    pub running_count: Option<u32>,
    #[serde(rename = "pendingCount", default)]
    pub pending_count: Option<u32>,
    #[serde(rename = "taskDefinition", default)]
    pub task_definition: Option<String>,
    #[serde(rename = "launchType", default)]
    pub launch_type: Option<String>,
    #[serde(rename = "platformVersion", default)]
    pub platform_version: Option<String>,
    #[serde(rename = "deployments", default)]
    pub deployments: Vec<Deployment>,
    #[serde(rename = "events", default)]
    pub events: Vec<ServiceEvent>,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListClusterArnsResponse {
    #[serde(rename = "clusterArns")]
    cluster_arns: Vec<String>,
    #[serde(rename = "nextToken", default)]
    next_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DescribeClustersResponse {
    #[serde(rename = "clusters")]
    clusters: Vec<Cluster>,
}

#[derive(Debug, Deserialize)]
struct ListServiceArnsResponse {
    #[serde(rename = "serviceArns")]
    service_arns: Vec<String>,
    #[serde(rename = "nextToken", default)]
    next_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DescribeServicesResponse {
    #[serde(rename = "services")]
    services: Vec<Service>,
}

/// Unified focused-item type so the renderer works across both
/// `clusters` and `services` tabs.
#[derive(Debug, Clone)]
pub enum Item {
    Cluster(Cluster),
    Service(Service),
}

impl Item {
    pub fn primary_label(&self) -> &str {
        match self {
            Item::Cluster(c) => &c.name,
            Item::Service(s) => &s.name,
        }
    }
    pub fn secondary_label(&self) -> String {
        match self {
            Item::Cluster(c) => {
                let status = c.status.as_deref().unwrap_or("?");
                let services = c.active_services.unwrap_or(0);
                let running = c.running_tasks.unwrap_or(0);
                format!("{status} · {services} svc · {running} tasks")
            }
            Item::Service(s) => {
                let status = s.status.as_deref().unwrap_or("?");
                let running = s.running_count.unwrap_or(0);
                let desired = s.desired_count.unwrap_or(0);
                format!("{status} · {running}/{desired}")
            }
        }
    }
    pub fn arn(&self) -> &str {
        match self {
            Item::Cluster(c) => &c.arn,
            Item::Service(s) => &s.arn,
        }
    }
}

pub fn list_clusters(region: Option<&str>) -> Result<Vec<Cluster>> {
    let arns = list_cluster_arns(region)?;
    if arns.is_empty() {
        return Ok(Vec::new());
    }
    // describe-clusters accepts up to 100 ARNs at once.
    let mut all = Vec::new();
    for chunk in arns.chunks(100) {
        let mut cmd = Command::new("aws");
        cmd.args(["ecs", "describe-clusters", "--clusters"]);
        for arn in chunk {
            cmd.arg(arn);
        }
        cmd.args(["--include", "STATISTICS", "--output", "json"]);
        if let Some(r) = region {
            cmd.args(["--region", r]);
        }
        let output = cmd
            .output()
            .with_context(|| "spawn `aws ecs describe-clusters`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "aws ecs describe-clusters failed: {}",
                stderr.trim()
            ));
        }
        let resp: DescribeClustersResponse = serde_json::from_slice(&output.stdout)
            .with_context(|| "parse describe-clusters JSON")?;
        all.extend(resp.clusters);
    }
    all.sort_by_key(|c| c.name.to_lowercase());
    Ok(all)
}

fn list_cluster_arns(region: Option<&str>) -> Result<Vec<String>> {
    let mut all = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let mut cmd = Command::new("aws");
        cmd.args(["ecs", "list-clusters", "--output", "json"]);
        if let Some(r) = region {
            cmd.args(["--region", r]);
        }
        if let Some(t) = &token {
            cmd.args(["--next-token", t]);
        }
        let output = cmd
            .output()
            .with_context(|| "spawn `aws ecs list-clusters`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("aws ecs list-clusters failed: {}", stderr.trim()));
        }
        let resp: ListClusterArnsResponse =
            serde_json::from_slice(&output.stdout).with_context(|| "parse list-clusters JSON")?;
        all.extend(resp.cluster_arns);
        match resp.next_token {
            Some(t) if !t.is_empty() => token = Some(t),
            _ => break,
        }
    }
    Ok(all)
}

pub fn list_services(cluster: &str, region: Option<&str>) -> Result<Vec<Service>> {
    let arns = list_service_arns(cluster, region)?;
    if arns.is_empty() {
        return Ok(Vec::new());
    }
    let mut all = Vec::new();
    // describe-services accepts up to 10 ARNs at once.
    for chunk in arns.chunks(10) {
        let mut cmd = Command::new("aws");
        cmd.args([
            "ecs",
            "describe-services",
            "--cluster",
            cluster,
            "--services",
        ]);
        for arn in chunk {
            cmd.arg(arn);
        }
        cmd.args(["--output", "json"]);
        if let Some(r) = region {
            cmd.args(["--region", r]);
        }
        let output = cmd
            .output()
            .with_context(|| "spawn `aws ecs describe-services`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "aws ecs describe-services failed: {}",
                stderr.trim()
            ));
        }
        let resp: DescribeServicesResponse = serde_json::from_slice(&output.stdout)
            .with_context(|| "parse describe-services JSON")?;
        all.extend(resp.services);
    }
    all.sort_by_key(|s| s.name.to_lowercase());
    Ok(all)
}

fn list_service_arns(cluster: &str, region: Option<&str>) -> Result<Vec<String>> {
    let mut all = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let mut cmd = Command::new("aws");
        cmd.args([
            "ecs",
            "list-services",
            "--cluster",
            cluster,
            "--output",
            "json",
        ]);
        if let Some(r) = region {
            cmd.args(["--region", r]);
        }
        if let Some(t) = &token {
            cmd.args(["--next-token", t]);
        }
        let output = cmd
            .output()
            .with_context(|| "spawn `aws ecs list-services`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "aws ecs list-services failed for cluster {cluster}: {}",
                stderr.trim()
            ));
        }
        let resp: ListServiceArnsResponse =
            serde_json::from_slice(&output.stdout).with_context(|| "parse list-services JSON")?;
        all.extend(resp.service_arns);
        match resp.next_token {
            Some(t) if !t.is_empty() => token = Some(t),
            _ => break,
        }
    }
    Ok(all)
}

/// Extract a clean task definition family + revision from an ARN
/// or a `family:revision` short form. `arn:aws:ecs:…:task-definition/api:42`
/// → `api:42`. Raw family:revision passes through unchanged.
pub fn task_definition_short(td: &str) -> String {
    td.rsplit('/').next().unwrap_or(td).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_describe_clusters_response() {
        let json = r#"{
            "clusters": [
                {
                    "clusterName": "prod-cluster",
                    "clusterArn": "arn:aws:ecs:us-east-1:1:cluster/prod-cluster",
                    "status": "ACTIVE",
                    "runningTasksCount": 12,
                    "pendingTasksCount": 0,
                    "activeServicesCount": 4,
                    "registeredContainerInstancesCount": 0,
                    "capacityProviders": ["FARGATE", "FARGATE_SPOT"]
                }
            ]
        }"#;
        let resp: DescribeClustersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.clusters.len(), 1);
        assert_eq!(resp.clusters[0].running_tasks, Some(12));
        assert_eq!(
            resp.clusters[0].capacity_providers,
            vec!["FARGATE", "FARGATE_SPOT"]
        );
    }

    #[test]
    fn parses_describe_services_response() {
        let json = r#"{
            "services": [
                {
                    "serviceName": "api",
                    "serviceArn": "arn:aws:ecs:us-east-1:1:service/prod-cluster/api",
                    "clusterArn": "arn:aws:ecs:us-east-1:1:cluster/prod-cluster",
                    "status": "ACTIVE",
                    "desiredCount": 3,
                    "runningCount": 3,
                    "pendingCount": 0,
                    "taskDefinition": "arn:aws:ecs:us-east-1:1:task-definition/api:42",
                    "launchType": "FARGATE",
                    "deployments": [
                        {
                            "id": "ecs-svc/123",
                            "status": "PRIMARY",
                            "taskDefinition": "arn:aws:ecs:us-east-1:1:task-definition/api:42",
                            "desiredCount": 3,
                            "runningCount": 3,
                            "rolloutState": "COMPLETED"
                        }
                    ],
                    "events": [
                        {
                            "id": "evt-1",
                            "createdAt": "2026-06-06T18:30:00Z",
                            "message": "(service api) has reached a steady state."
                        }
                    ]
                }
            ]
        }"#;
        let resp: DescribeServicesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.services.len(), 1);
        let svc = &resp.services[0];
        assert_eq!(svc.running_count, Some(3));
        assert_eq!(svc.deployments.len(), 1);
        assert_eq!(
            svc.deployments[0].rollout_state.as_deref(),
            Some("COMPLETED")
        );
        assert_eq!(svc.events.len(), 1);
    }

    #[test]
    fn task_definition_short_handles_arn() {
        assert_eq!(
            task_definition_short("arn:aws:ecs:us-east-1:1:task-definition/api:42"),
            "api:42"
        );
        assert_eq!(task_definition_short("api:42"), "api:42");
        assert_eq!(task_definition_short("api"), "api");
    }

    #[test]
    fn item_secondary_label_includes_task_counts() {
        let svc = Service {
            name: "api".into(),
            arn: "arn".into(),
            cluster_arn: "".into(),
            status: Some("ACTIVE".into()),
            desired_count: Some(3),
            running_count: Some(2),
            pending_count: Some(0),
            task_definition: None,
            launch_type: None,
            platform_version: None,
            deployments: vec![],
            events: vec![],
            created_at: None,
        };
        let label = Item::Service(svc).secondary_label();
        assert!(label.contains("ACTIVE"));
        assert!(label.contains("2/3"));
    }
}
