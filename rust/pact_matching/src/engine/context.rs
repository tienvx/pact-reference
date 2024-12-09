//! Traits and structs for dealing with the test context.

use std::panic::RefUnwindSafe;

use anyhow::anyhow;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use itertools::Itertools;
use tracing::{instrument, trace, Level};

use pact_models::matchingrules::{MatchingRule, MatchingRuleCategory};
use pact_models::path_exp::DocPath;
use pact_models::prelude::v4::{SynchronousHttp, V4Pact};
use pact_models::v4::interaction::V4Interaction;

use crate::engine::{ExecutionPlanNode, NodeResult, NodeValue};
use crate::matchers::Matches;

/// Context to store data for use in executing an execution plan.
#[derive(Clone, Debug)]
pub struct PlanMatchingContext {
  /// Pact the plan is for
  pub pact: V4Pact,
  /// Interaction that the plan id for
  pub interaction: Box<dyn V4Interaction + Send + Sync + RefUnwindSafe>,
  /// Stack of intermediate values (used by the pipeline operator and apply action)
  value_stack: Vec<Option<NodeResult>>,
  /// Matching rules to use
  matching_rules: MatchingRuleCategory,
}

impl PlanMatchingContext {
  /// Execute the action
  #[instrument(ret, skip(self), level = Level::TRACE)]
  pub fn execute_action(
    &self,
    action: &str,
    arguments: &Vec<ExecutionPlanNode>
  ) -> anyhow::Result<NodeResult> {
    trace!(%action, ?arguments, "Executing action");
    match action {
      "upper-case" => {
        let value = validate_one_arg(arguments, action)?;
        let result = value.as_string()
          .unwrap_or_default();
        Ok(NodeResult::VALUE(NodeValue::STRING(result.to_uppercase())))
      }
      "match:equality" => {
        let (first, second) = validate_two_args(arguments, action)?;
        let first = first.as_value().unwrap_or_default();
        let second = second.as_value().unwrap_or_default();
        first.matches_with(second, &MatchingRule::Equality, false)?;
        Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
      }
      "expect:empty" => {
        let arg = validate_one_arg(arguments, action)?;
        let arg_value = arg.as_value();
        if let Some(value) = &arg_value {
          match value {
            NodeValue::NULL => Ok(NodeResult::VALUE(NodeValue::BOOL(true))),
            NodeValue::STRING(s) => if s.is_empty() {
              Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
            } else {
              Err(anyhow!("Expected {:?} to be empty", value))
            }
            NodeValue::BOOL(b) => Ok(NodeResult::VALUE(NodeValue::BOOL(*b))),
            NodeValue::MMAP(m) => if m.is_empty() {
              Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
            } else {
              Err(anyhow!("Expected {:?} to be empty", value))
            }
            NodeValue::SLIST(l) => if l.is_empty() {
              Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
            } else {
              Err(anyhow!("Expected {:?} to be empty", value))
            },
            NodeValue::BARRAY(bytes) => if bytes.is_empty() {
              Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
            } else {
              Err(anyhow!("Expected byte array ({} bytes) to be empty", bytes.len()))
            },
            NodeValue::NAMESPACED(_, _) => { todo!("Not Implemented: Need a way to resolve NodeValue::NAMESPACED") }
            NodeValue::UINT(ui) => if *ui == 0 {
              Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
            } else {
              Err(anyhow!("Expected {:?} to be empty", value))
            }
          }
        } else {
          // TODO: If the parameter value is an error, this should return an error?
          Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
        }
      }
      "convert:UTF8" => {
        let arg = validate_one_arg(arguments, action)?;
        let arg_value = arg.as_value();
        if let Some(value) = &arg_value {
          match value {
            NodeValue::NULL => Ok(NodeResult::VALUE(NodeValue::STRING("".to_string()))),
            NodeValue::STRING(s) => Ok(NodeResult::VALUE(NodeValue::STRING(s.clone()))),
            NodeValue::BARRAY(b) => Ok(NodeResult::VALUE(NodeValue::STRING(String::from_utf8_lossy(b).to_string()))),
            _ => Err(anyhow!("convert:UTF8 can not be used with {}", value.value_type()))
          }
        } else {
          Ok(NodeResult::VALUE(NodeValue::STRING("".to_string())))
        }
      }
      "if" => {
        let (first, second) = validate_two_args(arguments, action)?;
        if first.is_truthy() {
          Ok(second)
        } else {
          Ok(first)
        }
      }
      "apply" => if let Some(value) = self.value_stack.last() {
        value.clone().ok_or_else(|| anyhow!("No value to apply (value on stack is empty)"))
      } else {
        Err(anyhow!("No value to apply (stack is empty)"))
      }
      _ => Err(anyhow!("'{}' is not a valid action", action))
    }
  }

  /// Push a result value onto the value stack
  pub fn push_result(&mut self, value: Option<NodeResult>) {
    self.value_stack.push(value);
  }

  /// Replace the top value of the stack with the new value
  pub fn update_result(&mut self, value: Option<NodeResult>) {
    if let Some(current) = self.value_stack.last_mut() {
      *current = value;
    } else {
      self.value_stack.push(value);
    }
  }

  /// Return the value on the top if the stack
  pub fn pop_result(&mut self) -> Option<NodeResult> {
    self.value_stack.pop().flatten()
  }

  /// If there is a matcher defined at the path in this context
  pub fn matcher_is_defined(&self, path: &DocPath) -> bool {
    let path = path.to_vec();
    let path_slice = path.iter().map(|p| p.as_str()).collect_vec();
    self.matching_rules.matcher_is_defined(path_slice.as_slice())
  }

  /// Creates a clone of this context, but with the matching rules set for the Request Method
  pub fn for_method(&self) -> Self {
    let matching_rules = if let Some(req_res) = self.interaction.as_v4_http() {
      req_res.request.matching_rules.rules_for_category("method").unwrap_or_default()
    } else {
      MatchingRuleCategory::default()
    };

    PlanMatchingContext {
      pact: self.pact.clone(),
      interaction: self.interaction.boxed_v4(),
      value_stack: vec![],
      matching_rules
    }
  }

  /// Creates a clone of this context, but with the matching rules set for the Request Path
  pub fn for_path(&self) -> Self {
    let matching_rules = if let Some(req_res) = self.interaction.as_v4_http() {
      req_res.request.matching_rules.rules_for_category("path").unwrap_or_default()
    } else {
      MatchingRuleCategory::default()
    };

    PlanMatchingContext {
      pact: self.pact.clone(),
      interaction: self.interaction.boxed_v4(),
      value_stack: vec![],
      matching_rules
    }
  }

  /// Creates a clone of this context, but with the matching rules set for the Request Query Parameters
  pub fn for_query(&self) -> Self {
    let matching_rules = if let Some(req_res) = self.interaction.as_v4_http() {
      req_res.request.matching_rules.rules_for_category("query").unwrap_or_default()
    } else {
      MatchingRuleCategory::default()
    };

    PlanMatchingContext {
      pact: self.pact.clone(),
      interaction: self.interaction.boxed_v4(),
      value_stack: vec![],
      matching_rules
    }
  }

  /// Creates a clone of this context, but with the matching rules set for the Request Headers
  pub fn for_headers(&self) -> Self {
    let matching_rules = if let Some(req_res) = self.interaction.as_v4_http() {
      req_res.request.matching_rules.rules_for_category("header").unwrap_or_default()
    } else {
      MatchingRuleCategory::default()
    };

    PlanMatchingContext {
      pact: self.pact.clone(),
      interaction: self.interaction.boxed_v4(),
      value_stack: vec![],
      matching_rules
    }
  }

  /// Creates a clone of this context, but with the matching rules set for the Request Body
  pub fn for_body(&self) -> Self {
    let matching_rules = if let Some(req_res) = self.interaction.as_v4_http() {
      req_res.request.matching_rules.rules_for_category("body").unwrap_or_default()
    } else {
      MatchingRuleCategory::default()
    };

    PlanMatchingContext {
      pact: self.pact.clone(),
      interaction: self.interaction.boxed_v4(),
      value_stack: vec![],
      matching_rules
    }
  }
}

fn validate_two_args(arguments: &Vec<ExecutionPlanNode>, action: &str) -> anyhow::Result<(NodeResult, NodeResult)> {
  if arguments.len() == 2 {
    let first = arguments[0].value().unwrap_or_default();
    let second = arguments[1].value().unwrap_or_default();
    Ok((first, second))
  } else {
    Err(anyhow!("{} requires two arguments, got {}", action, arguments.len()))
  }
}

fn validate_one_arg(arguments: &Vec<ExecutionPlanNode>, action: &str) -> anyhow::Result<NodeResult> {
  if arguments.len() > 1 {
    Err(anyhow!("{} takes only one argument, got {}", action, arguments.len()))
  } else if let Some(argument) = arguments.first() {
    Ok(argument.value().unwrap_or_default())
  } else {
    Err(anyhow!("{} requires one argument, got none", action))
  }
}

impl Default for PlanMatchingContext {
  fn default() -> Self {
    PlanMatchingContext {
      pact: Default::default(),
      interaction: Box::new(SynchronousHttp::default()),
      value_stack: vec![],
      matching_rules: Default::default()
    }
  }
}
