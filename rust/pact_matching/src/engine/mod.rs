//! Structs and traits to support a general matching engine

use std::panic::RefUnwindSafe;

use pact_models::v4::http_parts::HttpRequest;
use pact_models::v4::interaction::V4Interaction;
use pact_models::v4::pact::V4Pact;
use pact_models::v4::synch_http::SynchronousHttp;

#[derive(Clone, Debug, Default)]
pub enum PlanNodeType {
  #[default]
  CONTAINER
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionPlanNode {
  pub node_type: PlanNodeType,
  pub children: Vec<ExecutionPlanNode>
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionPlan {
  pub plan_root: ExecutionPlanNode
}

impl ExecutionPlan {
  pub fn str_form(&self) -> String {
    String::new()
  }

  pub fn pretty_form(&self) -> String {
    String::new()
  }
}

#[derive(Clone, Debug)]
pub struct PlanMatchingContext {
  pub pact: V4Pact,
  pub interaction: Box<dyn V4Interaction + Send + Sync + RefUnwindSafe>
}

impl Default for PlanMatchingContext {
  fn default() -> Self {
    PlanMatchingContext {
      pact: Default::default(),
      interaction: Box::new(SynchronousHttp::default())
    }
  }
}

pub fn build_request_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlan> {
  Ok(ExecutionPlan::default())
}

pub fn execute_request_plan(
  plan: &ExecutionPlan,
  actual: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlan> {
  Ok(ExecutionPlan::default())
}

#[cfg(test)]
mod tests {
  use expectest::prelude::*;
  use serde_json::json;
  use pact_models::bodies::OptionalBody;
  use pact_models::v4::http_parts::HttpRequest;
  use pretty_assertions::assert_eq;

  use crate::engine::{build_request_plan, execute_request_plan, ExecutionPlan, PlanMatchingContext};

  #[test]
  fn simple_match_request_test() -> anyhow::Result<()> {
    let request = HttpRequest {
      method: "POST".to_string(),
      path: "/test".to_string(),
      query: None,
      headers: None,
      body: OptionalBody::from(&json!({
        "b": "22"
      })),
      matching_rules: Default::default(),
      generators: Default::default(),
    };
    let expected_request = HttpRequest {
      method: "POST".to_string(),
      path: "/test".to_string(),
      query: None,
      headers: None,
      body: OptionalBody::from(&json!({
        "a": 100,
        "b": 200.1
      })),
      matching_rules: Default::default(),
      generators: Default::default(),
    };
    let context = PlanMatchingContext::default();
    let plan = build_request_plan(&expected_request, &context)?;

    assert_eq!(plan.pretty_form(), r#"(
      :request (
        :method (
          %match:equality (
            (%upper-case ($.method)), "GET"
          )
        ),
        :path (
          %match:equality ($.path, "/test")
        ),
        :"query parameters" (
          %expect:empty ($.query)
        ),
        :body (
          %if (
            %match:equality (%content-type (), "application/json"),
            :body:$ (
              :body:$:a (
                %if (
                  %expect:present ($.body."$.a"),
                  %match:equality ($.body."$.a", 100)
                )
              ),
              :body:$:b (
                %if (
                  %expect:present ($.body."$.b"),
                  %match:equality ($.body."$.b", 200.1)
                )
              )
            )
          )
        )
      )
    )"#);

    let executed_plan = execute_request_plan(&plan, &request, &context)?;
    assert_eq!(executed_plan.pretty_form(), r#"(
      :request (
        :method (
          %match:equality (
            (%upper-case ($.method ~ "GET")), "GET" ~ OK
          )
        ),
        :path (
          %match:equality ($.path ~ "/test", "/test") ~ OK
        ),
        :"query parameters" (
          %expect:empty ($.query ~ {}) ~ OK
        ),
        :body (
          %if (
            %match:equality (%content-type () ~ "application/json", "application/json") ~ OK,
            :body:$ (
              :body:$:a (
                %if (
                  %expect:present ($.body."$.a" ~ NULL) ~ ERROR(Expected attribute "$.a" but it was missing),
                  %match:equality ($.body."$.a", 100) ~ NULL
                )
              ),
              :body:$:b (
                %if (
                  %expect:present ($.body."$.b" ~ "22") ~ OK,
                  %match:equality ($.body."$.b" ~ "22", 200.1) ~ ERROR(Expected attribute "$.b" to equal "22" (String) but it was 200.1 (Double))
                )
              )
            )
          )
        )
      )
    )"#);

    Ok(())
  }
}
