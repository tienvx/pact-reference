//! Traits and structs for dealing with the test context.

use std::collections::HashSet;
use std::panic::RefUnwindSafe;

use anyhow::{anyhow, Error};
use itertools::Itertools;
use serde_json::Value;
use tracing::{instrument, trace, Level};

use pact_models::matchingrules::{MatchingRule, MatchingRuleCategory};
use pact_models::path_exp::DocPath;
use pact_models::prelude::v4::{SynchronousHttp, V4Pact};
use pact_models::v4::interaction::V4Interaction;

use crate::engine::{ExecutionPlanNode, NodeResult, NodeValue, walk_tree};
use crate::engine::value_resolvers::ValueResolver;
use crate::json::type_of;
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
  /// If extra keys/values are allowed (and ignored)
  pub allow_unexpected_entries: bool
}

impl PlanMatchingContext {
  /// Execute the action
  #[instrument(ret, skip_all, level = Level::TRACE, fields(action, path, node))]
  pub fn execute_action(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    path: &[String]
  ) -> ExecutionPlanNode {
    trace!(%action, "Executing action");

    let mut action_path = path.to_vec();
    action_path.push(action.to_string());

    match action {
      "upper-case" => self.execute_upper_case(action, value_resolver, node, &action_path),
      "match:equality" => {
        match self.validate_two_args(node, action, value_resolver, &action_path) {
          Ok((first_node, second_node)) => {
            let first = first_node.value()
              .unwrap_or_default()
              .as_value()
              .unwrap_or_default();
            let second = second_node.value()
              .unwrap_or_default()
              .as_value()
              .unwrap_or_default();
            match first.matches_with(second, &MatchingRule::Equality, false) {
              Ok(_) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::VALUE(NodeValue::BOOL(true))),
                  children: vec![first_node, second_node]
                }
              }
              Err(err) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![first_node, second_node]
                }
              }
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      "expect:empty" => {
        match self.validate_one_arg(node, action, value_resolver, &action_path) {
          Ok(value) => {
            let arg_value = value.value().unwrap_or_default().as_value();
            let result = if let Some(value) = &arg_value {
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
                },
                NodeValue::JSON(json) => match json {
                  Value::Null => Ok(NodeResult::VALUE(NodeValue::BOOL(true))),
                  Value::String(s) => if s.is_empty() {
                    Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
                  } else {
                    Err(anyhow!("Expected JSON String ({}) to be empty", json))
                  }
                  Value::Array(a) => if a.is_empty() {
                    Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
                  } else {
                    Err(anyhow!("Expected JSON Array ({}) to be empty", json))
                  }
                  Value::Object(o) => if o.is_empty() {
                    Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
                  } else {
                    Err(anyhow!("Expected JSON Object ({}) to be empty", json))
                  }
                  _ => Err(anyhow!("Expected json ({}) to be empty", json))
                }
              }
            } else {
              // TODO: If the parameter value is an error, this should return an error?
              Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
            };
            match result {
              Ok(result) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(result),
                  children: vec![value]
                }
              }
              Err(err) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![value]
                }
              }
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      "convert:UTF8" => {
        match self.validate_one_arg(node, action, value_resolver, &action_path) {
          Ok(value) => {
            let arg_value = value.value().unwrap_or_default().as_value();
            let result = if let Some(value) = &arg_value {
              match value {
                NodeValue::NULL => Ok(NodeResult::VALUE(NodeValue::STRING("".to_string()))),
                NodeValue::STRING(s) => Ok(NodeResult::VALUE(NodeValue::STRING(s.clone()))),
                NodeValue::BARRAY(b) => Ok(NodeResult::VALUE(NodeValue::STRING(String::from_utf8_lossy(b).to_string()))),
                _ => Err(anyhow!("convert:UTF8 can not be used with {}", value.value_type()))
              }
            } else {
              Ok(NodeResult::VALUE(NodeValue::STRING("".to_string())))
            };
            match result {
              Ok(result) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(result),
                  children: vec![value]
                }
              }
              Err(err) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![value]
                }
              }
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      "if" => {
        if let Some(first_node) = node.children.first() {
          match walk_tree(action_path.as_slice(), first_node, value_resolver, self) {
            Ok(first) => {
              let node_result = first.value().unwrap_or_default();
              if !node_result.is_truthy() {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(node_result),
                  children: vec![first]
                }
              } else if let Some(second_node) = node.children.get(1) {
                match walk_tree(action_path.as_slice(), second_node, value_resolver, self) {
                  Ok(second) => {
                    let second_result = second.value().unwrap_or_default();
                    ExecutionPlanNode {
                      node_type: node.node_type.clone(),
                      result: Some(second_result),
                      children: vec![first, second]
                    }
                  }
                  Err(err) => {
                    ExecutionPlanNode {
                      node_type: node.node_type.clone(),
                      result: Some(NodeResult::ERROR(err.to_string())),
                      children: vec![first, second_node.clone()]
                    }
                  }
                }
              } else {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(node_result),
                  children: vec![first]
                }
              }
            }
            Err(err) => {
              ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(NodeResult::ERROR(err.to_string())),
                children: node.children.clone()
              }
            }
          }
        } else {
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR("'if' action requires at least one argument".to_string())),
            children: node.children.clone()
          }
        }
      }
      "apply" => if let Some(value) = self.value_stack.last() {
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: value.clone(),
          children: node.children.clone()
        }
      } else {
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::ERROR("No value to apply (stack is empty)".to_string())),
          children: node.children.clone()
        }
      }
      "push" => {
        let last_value = self.value_stack.last().cloned();
        if let Some(value) = last_value {
          self.value_stack.push(value.clone());
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: value,
            children: node.children.clone()
          }
        } else {
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR("No value to push (value is empty)".to_string())),
            children: node.children.clone()
          }
        }
      }
      "pop" => if let Some(_value) = self.value_stack.pop() {
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: self.value_stack.last().cloned().flatten(),
          children: node.children.clone()
        }
      } else {
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::ERROR("No value to pop (stack is empty)".to_string())),
          children: node.children.clone()
        }
      }
      "json:parse" => {
        match self.validate_one_arg(node, action, value_resolver, &action_path) {
          Ok(value) => {
            let arg_value = value.value().unwrap_or_default().as_value();
            let result = if let Some(value) = &arg_value {
              match value {
                NodeValue::NULL => Ok(NodeResult::VALUE(NodeValue::NULL)),
                NodeValue::STRING(s) => serde_json::from_str(s.as_str())
                  .map(|json| NodeResult::VALUE(NodeValue::JSON(json)))
                  .map_err(|err| anyhow!("json parse error - {}", err)),
                NodeValue::BARRAY(b) => serde_json::from_slice(b.as_slice())
                  .map(|json| NodeResult::VALUE(NodeValue::JSON(json)))
                  .map_err(|err| anyhow!("json parse error - {}", err)),
                _ => Err(anyhow!("json:parse can not be used with {}", value.value_type()))
              }
            } else {
              Ok(NodeResult::VALUE(NodeValue::NULL))
            };
            match result {
              Ok(result) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(result),
                  children: vec![value]
                }
              }
              Err(err) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![value]
                }
              }
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      "json:expect:empty" => {
        match self.validate_two_args(node, action, value_resolver, &action_path) {
          Ok((first_node, second_node)) => {
            let result1 = first_node.value().unwrap_or_default();
            let expected_json_type = match result1.as_string() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("'{}' is not a valid JSON type", result1))),
                  children: vec![first_node, second_node]
                }
              }
              Some(str) => str
            };
            let result2 = second_node.value().unwrap_or_default();
            let value = match result2.as_value() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON value, but got {}", result2))),
                  children: vec![first_node, second_node]
                }
              }
              Some(value) => value
            };
            let json_value = match value {
              NodeValue::JSON(json) => json,
              _ => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON value, but got {:?}", value))),
                  children: vec![first_node, second_node]
                }
              }
            };
            if let Err(err) = json_check_type(expected_json_type, &json_value) {
              return ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(NodeResult::ERROR(err.to_string())),
                children: vec![first_node, second_node]
              }
            };
            let result = match &json_value {
              Value::Null => Ok(NodeResult::VALUE(NodeValue::BOOL(true))),
              Value::String(s) => if s.is_empty() {
                Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
              } else {
                Err(anyhow!("Expected JSON String ({}) to be empty", json_value))
              }
              Value::Array(a) => if a.is_empty() {
                Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
              } else {
                Err(anyhow!("Expected JSON Array ({}) to be empty", json_value))
              }
              Value::Object(o) => if o.is_empty() {
                Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
              } else {
                Err(anyhow!("Expected JSON Object ({}) to be empty", json_value))
              }
              _ => Err(anyhow!("Expected json ({}) to be empty", json_value))
            };
            match result {
              Ok(result) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(result),
                  children: vec![first_node, second_node]
                }
              }
              Err(err) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![first_node, second_node]
                }
              }
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      "json:match:length" => {
        match self.validate_three_args(node, action, value_resolver, &action_path) {
          Ok((first_node, second_node, third_node)) => {
            let result1 = first_node.value().unwrap_or_default();
            let expected_json_type = match result1.as_string() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("'{}' is not a valid JSON type", result1))),
                  children: vec![first_node, second_node, third_node]
                }
              }
              Some(str) => str
            };
            let result2 = second_node.value().unwrap_or_default();
            let expected_length = match result2.as_number() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("'{}' is not a valid number", result2))),
                  children: vec![first_node, second_node, third_node]
                }
              }
              Some(length) => length
            };
            let result3 = third_node.value().unwrap_or_default();
            let value = match result3.as_value() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON value, but got {}", result3))),
                  children: vec![first_node, second_node, third_node]
                }
              }
              Some(value) => value
            };
            let json_value = match value {
              NodeValue::JSON(json) => json,
              _ => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON value, but got {:?}", value))),
                  children: vec![first_node, second_node, third_node]
                }
              }
            };
            if let Err(err) = json_check_type(expected_json_type, &json_value) {
              return ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(NodeResult::ERROR(err.to_string())),
                children: vec![first_node, second_node, third_node]
              }
            }
            if let Err(err) = json_check_length(expected_length as usize, &json_value) {
              return ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(NodeResult::ERROR(err.to_string())),
                children: vec![first_node, second_node, third_node]
              }
            }
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::VALUE(NodeValue::BOOL(true))),
              children: vec![first_node, second_node, third_node]
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      "json:expect:entries" => {
        match self.validate_three_args(node, action, value_resolver, &action_path) {
          Ok((first_node, second_node, third_node)) => {
            let result1 = first_node.value().unwrap_or_default();
            let expected_json_type = match result1.as_string() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("'{}' is not a valid JSON type", result1))),
                  children: vec![first_node, second_node, third_node]
                }
              }
              Some(str) => str
            };
            let result2 = second_node.value().unwrap_or_default();
            let expected_keys = match result2.as_slist() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("'{}' is not a list of Strings", result2))),
                  children: vec![first_node, second_node, third_node]
                }
              }
              Some(list) => list.iter()
                .cloned()
                .collect::<HashSet<_>>()
            };
            let result3 = third_node.value().unwrap_or_default();
            let value = match result3.as_value() {
              None => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON value, but got {}", result3))),
                  children: vec![first_node, second_node, third_node]
                }
              }
              Some(value) => value
            };
            let json_value = match &value {
              NodeValue::JSON(json) => json,
              _ => {
                return ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON value, but got {:?}", value))),
                  children: vec![first_node, second_node, third_node]
                }
              }
            };
            if let Err(err) = json_check_type(expected_json_type, json_value) {
              return ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(NodeResult::ERROR(err.to_string())),
                children: vec![first_node, second_node, third_node]
              }
            }

            match json_value {
              Value::Object(o) => {
                let actual_keys = o.keys()
                  .cloned()
                  .collect::<HashSet<_>>();
                let diff = &expected_keys - &actual_keys;
                if diff.is_empty() {
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(NodeResult::VALUE(NodeValue::BOOL(true))),
                    children: vec![first_node, second_node, third_node]
                  }
                } else {
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(
                      NodeResult::ERROR(
                        format!("The following expected entries were missing from the actual Object: {}",
                                diff.iter().join(", "))
                      )
                    ),
                    children: vec![first_node, second_node, third_node]
                  }
                }
              }
              _ => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(format!("Was expecting a JSON Object, but got {:?}", json_value))),
                  children: vec![first_node, second_node, third_node]
                }
              }
            }
          }
          Err(err) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            }
          }
        }
      }
      _ => {
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::ERROR(format!("'{}' is not a valid action", action))),
          children: node.children.clone()
        }
      }
    }
  }

  fn execute_upper_case(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    match self.validate_one_arg(node, action, value_resolver, &action_path) {
      Ok(value) => {
        let result = value.value()
          .unwrap_or_default()
          .as_string()
          .unwrap_or_default();
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::VALUE(NodeValue::STRING(result.to_uppercase()))),
          children: vec![value]
        }
      }
      Err(err) => {
        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::ERROR(err.to_string())),
          children: node.children.clone()
        }
      }
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

  /// Return the current stack value
  pub fn stack_value(&self) -> Option<NodeResult> {
    self.value_stack.last().cloned().flatten()
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
      matching_rules,
      allow_unexpected_entries: false
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
      matching_rules,
      allow_unexpected_entries: false
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
      matching_rules,
      allow_unexpected_entries: false
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
      matching_rules,
      allow_unexpected_entries: false
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
      matching_rules,
      allow_unexpected_entries: self.allow_unexpected_entries
    }
  }

  fn validate_one_arg(
    &mut self,
    node: &ExecutionPlanNode,
    action: &str,
    value_resolver: &dyn ValueResolver,
    path: &Vec<String>
  ) -> anyhow::Result<ExecutionPlanNode> {
    if node.children.len() > 1 {
      Err(anyhow!("{} takes only one argument, got {}", action, node.children.len()))
    } else if let Some(argument) = node.children.first() {
      walk_tree(path.as_slice(), argument, value_resolver, self)
    } else {
      Err(anyhow!("{} requires one argument, got none", action))
    }
  }

  fn validate_two_args(
    &mut self,
    node: &ExecutionPlanNode,
    action: &str,
    value_resolver: &dyn ValueResolver,
    path: &Vec<String>
  ) -> anyhow::Result<(ExecutionPlanNode, ExecutionPlanNode)> {
    if node.children.len() == 2 {
      let first = walk_tree(path.as_slice(), &node.children[0], value_resolver, self)?;
      let second = walk_tree(path.as_slice(), &node.children[1], value_resolver, self)?;
      Ok((first, second))
    } else {
      Err(anyhow!("Action '{}' requires two arguments, got {}", action, node.children.len()))
    }
  }

  fn validate_three_args(
    &mut self,
    node: &ExecutionPlanNode,
    action: &str,
    value_resolver: &dyn ValueResolver,
    path: &Vec<String>
  ) -> anyhow::Result<(ExecutionPlanNode, ExecutionPlanNode, ExecutionPlanNode)> {
    if node.children.len() == 3 {
      let first = walk_tree(path.as_slice(), &node.children[0], value_resolver, self)?;
      let second = walk_tree(path.as_slice(), &node.children[1], value_resolver, self)?;
      let third = walk_tree(path.as_slice(), &node.children[2], value_resolver, self)?;
      Ok((first, second, third))
    } else {
      Err(anyhow!("Action '{}' requires three arguments, got {}", action, node.children.len()))
    }
  }
}

fn json_check_length(length: usize, json: &Value) -> anyhow::Result<()> {
  match json {
    Value::Array(a) => if a.len() == length {
      Ok(())
    } else {
      Err(anyhow!("Was expecting a length of {}, but actual length is {}", length, a.len()))
    }
    Value::Object(o) => if o.len() == length {
      Ok(())
    } else {
      Err(anyhow!("Was expecting a length of {}, but actual length is {}", length, o.len()))
    }
    _ => Ok(())
  }
}

fn json_check_type(expected_type: String, json_value: &Value) -> anyhow::Result<()> {
  match expected_type.as_str() {
    "NULL" => json_value.as_null()
      .ok_or_else(|| anyhow!("Was expecting a JSON NULL but got a {}", type_of(&json_value))),
    "BOOL" => json_value.as_bool()
      .ok_or_else(|| anyhow!("Was expecting a JSON Bool but got a {}", type_of(&json_value)))
      .map(|_| ()),
    "NUMBER" => json_value.as_number()
      .ok_or_else(|| anyhow!("Was expecting a JSON Number but got a {}", type_of(&json_value)))
      .map(|_| ()),
    "STRING" => json_value.as_str()
      .ok_or_else(|| anyhow!("Was expecting a JSON String but got a {}", type_of(&json_value)))
      .map(|_| ()),
    "ARRAY" => json_value.as_array()
      .ok_or_else(|| anyhow!("Was expecting a JSON Array but got a {}", type_of(&json_value)))
      .map(|_| ()),
    "OBJECT" => json_value.as_object()
      .ok_or_else(|| anyhow!("Was expecting a JSON Object but got a {}", type_of(&json_value)))
      .map(|_| ()),
    _ => Err(anyhow!("'{}' is not a valid JSON type", expected_type))
  }
}

impl Default for PlanMatchingContext {
  fn default() -> Self {
    PlanMatchingContext {
      pact: Default::default(),
      interaction: Box::new(SynchronousHttp::default()),
      value_stack: vec![],
      matching_rules: Default::default(),
      allow_unexpected_entries: false
    }
  }
}
