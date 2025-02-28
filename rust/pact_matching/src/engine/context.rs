//! Traits and structs for dealing with the test context.

use std::collections::{HashSet, VecDeque};
use std::iter::once;
use std::panic::RefUnwindSafe;

use anyhow::anyhow;
use itertools::Itertools;
use serde_json::{json, Value};
use tracing::{instrument, trace, Level, debug};

use pact_models::matchingrules::{MatchingRule, MatchingRuleCategory, RuleList};
use pact_models::path_exp::DocPath;
use pact_models::prelude::v4::{SynchronousHttp, V4Pact};
use pact_models::v4::interaction::V4Interaction;

use crate::engine::{ExecutionPlanNode, NodeResult, NodeValue, walk_tree};
use crate::engine::value_resolvers::ValueResolver;
use crate::headers::{parse_charset_parameters, strip_whitespace};
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
  pub value_stack: Vec<Option<NodeResult>>,
  /// Matching rules to use
  pub matching_rules: MatchingRuleCategory,
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

    if action.starts_with("match:") {
      match action.strip_prefix("match:") {
        None => {
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR(format!("'{}' is not a valid action", action))),
            children: node.children.clone()
          }
        }
        Some(matcher) => self.execute_match(action, matcher, value_resolver, node, &action_path)
            .unwrap_or_else(|node| node)
      }
    } else {
      match action {
        "upper-case" => self.execute_change_case(action, value_resolver, node, &action_path, true),
        "lower-case" => self.execute_change_case(action, value_resolver, node, &action_path, false),
        "to-string" => self.execute_to_string(action, value_resolver, node, &action_path),
        "expect:empty" => self.execute_expect_empty(action, value_resolver, node, &action_path),
        "convert:UTF8" => self.execute_convert_utf8(action, value_resolver, node, &action_path),
        "if" => self.execute_if(value_resolver, node, &action_path),
        "tee" => self.execute_tee(value_resolver, node, &action_path),
        "apply" => self.execute_apply(node),
        "push" => self.execute_push(node),
        "pop" => self.execute_pop(node),
        "json:parse" => self.execute_json_parse(action, value_resolver, node, &action_path),
        "json:expect:empty" => self.execute_json_expect_empty(action, value_resolver, node, &action_path),
        "json:match:length" => self.execute_json_match_length(action, value_resolver, node, &action_path),
        "json:expect:entries" => self.execute_json_expect_entries(action, value_resolver, node, &action_path),
        "check:exists" => self.execute_check_exists(action, value_resolver, node, &action_path),
        "expect:entries" => self.execute_check_entries(action, value_resolver, node, &action_path),
        "expect:only-entries" => self.execute_check_entries(action, value_resolver, node, &action_path),
        "join" => self.execute_join(action, value_resolver, node, &action_path),
        "join-with" => self.execute_join(action, value_resolver, node, &action_path),
        "error" => self.execute_error(action, value_resolver, node, &action_path),
        "header:parse" => self.execute_header_parse(action, value_resolver, node, &action_path),
        _ => {
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR(format!("'{}' is not a valid action", action))),
            children: node.children.clone()
          }
        }
      }
    }
  }

  fn execute_json_expect_entries(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
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

  fn execute_json_match_length(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
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

  fn execute_json_expect_empty(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
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

  fn execute_json_parse(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
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

  fn execute_pop(&mut self, node: &ExecutionPlanNode) -> ExecutionPlanNode {
    if let Some(_value) = self.value_stack.pop() {
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
  }

  fn execute_push(&mut self, node: &ExecutionPlanNode) -> ExecutionPlanNode {
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

  fn execute_apply(&mut self, node: &ExecutionPlanNode) -> ExecutionPlanNode {
    if let Some(value) = self.value_stack.last() {
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
  }

  fn execute_if(
    &mut self,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    if let Some(first_node) = node.children.first() {
      match walk_tree(action_path.as_slice(), first_node, value_resolver, self) {
        Ok(first) => {
          let node_result = first.value().unwrap_or_default();
          let mut children = node.children.clone();
          children[0] = first.clone();
          if !node_result.is_truthy() {
            if node.children.len() > 2 {
              match walk_tree(action_path.as_slice(), &node.children[2], value_resolver, self) {
                Ok(else_node) => {
                  children[2] = else_node.clone();
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: else_node.result.clone(),
                    children
                  }
                }
                Err(err) => {
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(NodeResult::ERROR(err.to_string())),
                    children
                  }
                }
              }
            } else {
              ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(node_result),
                children
              }
            }
          } else if let Some(second_node) = node.children.get(1) {
            match walk_tree(action_path.as_slice(), second_node, value_resolver, self) {
              Ok(second) => {
                let second_result = second.value().unwrap_or_default();
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(second_result),
                  children: vec![first, second].iter()
                    .chain(node.children.iter().dropping(2))
                    .cloned()
                    .collect()
                }
              }
              Err(err) => {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![first, second_node.clone()].iter()
                    .chain(node.children.iter().dropping(2))
                    .cloned()
                    .collect()
                }
              }
            }
          } else {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(node_result),
              children: vec![first].iter().chain(node.children.iter().dropping(1))
                .cloned()
                .collect()
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

  fn execute_tee(
    &mut self,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    if let Some(first_node) = node.children.first() {
      match walk_tree(action_path.as_slice(), first_node, value_resolver, self) {
        Ok(first) => {
          let mut result = NodeResult::OK;
          self.push_result(first.result.clone());
          let mut child_results = vec![first.clone()];
          for child in node.children.iter().dropping(1) {
            match walk_tree(&action_path, &child, value_resolver, self) {
              Ok(value) => {
                result = result.or(&value.result);
                child_results.push(value.clone());
              }
              Err(err) => {
                let node_result = NodeResult::ERROR(err.to_string());
                result = result.or(&Some(node_result.clone()));
                child_results.push(child.clone_with_result(node_result));
              }
            }
          }

          self.pop_result();
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(result),
            children: child_results
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
        result: Some(NodeResult::OK),
        children: node.children.clone()
      }
    }
  }

  fn execute_convert_utf8(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
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

  fn execute_expect_empty(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    match self.validate_args(1, 1, node, action, value_resolver, &action_path) {
      Ok((values, optional)) => {
        let first = values.first().unwrap().value().unwrap_or_default();
        if let NodeResult::ERROR(err) = first  {
          ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR(err.to_string())),
            children: values.iter().chain(optional.iter()).cloned().collect()
          }
        } else {
          let arg_value = first.as_value();
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
                Err(anyhow!("Expected {} to be empty", value))
              }
              NodeValue::SLIST(l) => if l.is_empty() {
                Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
              } else {
                Err(anyhow!("Expected {} to be empty", value))
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
              },
              NodeValue::ENTRY(_, _) =>  Ok(NodeResult::VALUE(NodeValue::BOOL(false))),
              NodeValue::LIST(l) => if l.is_empty() {
                Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
              } else {
                Err(anyhow!("Expected {} to be empty", value))
              }
            }
          } else {
            Ok(NodeResult::VALUE(NodeValue::BOOL(true)))
          };
          match result {
            Ok(result) => {
              ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(result),
                children: values.iter().chain(optional.iter()).cloned().collect()
              }
            }
            Err(err) => {
              debug!("expect:empty failed with an error: {}", err);
              if optional.len() > 0 {
                if let Ok(value) = walk_tree(action_path.as_slice(), &optional[0], value_resolver, self) {
                  let message = value.value().unwrap_or_default().as_string().unwrap_or_default();
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(NodeResult::ERROR(message)),
                    children: values.iter().chain(once(&value)).cloned().collect()
                  }
                } else {
                  // There was an error generating the optional message, so just return the
                  // original error
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(NodeResult::ERROR(err.to_string())),
                    children: values.iter().chain(optional.iter()).cloned().collect()
                  }
                }
              } else {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: values.iter().chain(optional.iter()).cloned().collect()
                }
              }
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

  fn execute_match(
    &mut self,
    action: &str,
    matcher: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> Result<ExecutionPlanNode, ExecutionPlanNode> {
    match self.validate_three_args(node, action, value_resolver, &action_path) {
      Ok((first_node, second_node, third_node)) => {
        let exepected_value = first_node.value()
          .unwrap_or_default()
          .value_or_error()
          .map_err(|err| {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: vec![first_node.clone(), second_node.clone(), third_node.clone()]
            }
          })?;
        let actual_value = second_node.value()
          .unwrap_or_default()
          .value_or_error()
          .map_err(|err| {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: vec![first_node.clone(), second_node.clone(), third_node.clone()]
            }
          })?;
        let matcher_params = third_node.value()
          .unwrap_or_default()
          .value_or_error()
          .map_err(|err| {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: vec![first_node.clone(), second_node.clone(), third_node.clone()]
            }
          })?
          .as_json()
          .unwrap_or_default();
        match MatchingRule::create(matcher, &matcher_params) {
          Ok(rule) => {
            match exepected_value.matches_with(actual_value, &rule, false) {
              Ok(_) => {
                Ok(ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::VALUE(NodeValue::BOOL(true))),
                  children: vec![first_node.clone(), second_node.clone(), third_node.clone()]
                })
              }
              Err(err) => {
                Err(ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: vec![first_node.clone(), second_node.clone(), third_node.clone()]
                })
              }
            }
          }
          Err(err) => {
            Err(ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: node.children.clone()
            })
          }
        }
      }
      Err(err) => {
        Err(ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::ERROR(err.to_string())),
          children: node.children.clone()
        })
      }
    }
  }

  fn execute_change_case(
    &mut self,
    _action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>,
    upper_case: bool
  ) -> ExecutionPlanNode {
    let (children, values) = match self.evaluate_children(value_resolver, node, action_path) {
      Ok(value) => value,
      Err(value) => return value
    };

    let results = values.iter()
      .map(|v| {
        if upper_case {
          match v {
            NodeValue::STRING(s) => NodeValue::STRING(s.to_uppercase()),
            NodeValue::SLIST(list) => NodeValue::SLIST(list.iter().map(|s| s.to_uppercase()).collect()),
            _ => v.clone()
          }
        } else {
          match v {
            NodeValue::STRING(s) => NodeValue::STRING(s.to_lowercase()),
            NodeValue::SLIST(list) => NodeValue::SLIST(list.iter().map(|s| s.to_lowercase()).collect()),
            _ => v.clone()
          }
        }
      })
      .collect_vec();
    let result = if results.len() == 1 {
      results[0].clone()
    } else {
      NodeValue::LIST(results)
    };
    ExecutionPlanNode {
      node_type: node.node_type.clone(),
      result: Some(NodeResult::VALUE(result)),
      children
    }
  }

  fn execute_to_string(
    &mut self,
    _action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    let (children, values) = match self.evaluate_children(value_resolver, node, action_path) {
      Ok(value) => value,
      Err(value) => return value
    };

    let results = values.iter()
      .map(|v| {
        match v {
          NodeValue::STRING(_) => v.clone(),
          NodeValue::SLIST(_) => v.clone(),
          NodeValue::JSON(json) => match json {
            Value::String(s) => NodeValue::STRING(s.clone()),
            _ => NodeValue::STRING(json.to_string())
          }
          _ => NodeValue::STRING(v.str_form())
        }
      })
      .collect_vec();
    let result = if results.len() == 1 {
      results[0].clone()
    } else {
      NodeValue::LIST(results)
    };
    ExecutionPlanNode {
      node_type: node.node_type.clone(),
      result: Some(NodeResult::VALUE(result)),
      children
    }
  }

  fn execute_check_exists(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    match self.validate_one_arg(node, action, value_resolver, &action_path) {
      Ok(value) => {
        let result = if let NodeResult::VALUE(value) = value.value().unwrap_or_default() {
          match value {
            NodeValue::NULL => NodeResult::VALUE(NodeValue::BOOL(false)),
            _ => NodeResult::VALUE(NodeValue::BOOL(true))
          }
        } else {
          NodeResult::VALUE(NodeValue::BOOL(false))
        };
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

  /// Select the best matcher to use for the given path
  pub fn select_best_matcher(&self, path: &DocPath) -> RuleList {
    let path = path.to_vec();
    let path_slice = path.iter().map(|p| p.as_str()).collect_vec();
    self.matching_rules.select_best_matcher(path_slice.as_slice())
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

  fn validate_args(
    &mut self,
    required: usize,
    optional: usize,
    node: &ExecutionPlanNode,
    action: &str,
    value_resolver: &dyn ValueResolver,
    path: &Vec<String>
  ) -> anyhow::Result<(Vec<ExecutionPlanNode>, Vec<ExecutionPlanNode>)> {
    if node.children.len() < required {
      Err(anyhow!("{} requires {} arguments, got {}", action, required, node.children.len()))
    } else if node.children.len() > required + optional {
      Err(anyhow!("{} supports at most {} arguments, got {}", action, optional, node.children.len()))
    } else {
      let mut required_args = vec![];
      for child in node.children.iter().take(required) {
        let value = walk_tree(path.as_slice(), child, value_resolver, self)?;
        required_args.push(value);
      }
      Ok((required_args, node.children.iter().dropping(required).cloned().collect()))
    }
  }

  fn execute_join(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    path: &Vec<String>
  ) -> ExecutionPlanNode {
    let (children, str_values) = match self.evaluate_children(value_resolver, node, path) {
      Ok((children, values)) => {
        (children, values.iter().flat_map(|v| {
          match v {
            NodeValue::STRING(s) => vec![s.clone()],
            NodeValue::BOOL(b) => vec![b.to_string()],
            NodeValue::MMAP(_) => vec![v.str_form()],
            NodeValue::SLIST(list) => list.clone(),
            NodeValue::BARRAY(_) => vec![v.str_form()],
            NodeValue::NAMESPACED(_, _) => vec![v.str_form()],
            NodeValue::UINT(u) => vec![u.to_string()],
            NodeValue::JSON(json) => vec![json.to_string()],
            _ => vec![]
          }
        }).collect_vec())
      },
      Err(value) => return value
    };

    let result = if action == "join-with" && !str_values.is_empty() {
      let first = &str_values[0];
      str_values.iter().dropping(1).join(first.as_str())
    } else {
      str_values.iter().join("")
    };

    ExecutionPlanNode {
      node_type: node.node_type.clone(),
      result: Some(NodeResult::VALUE(NodeValue::STRING(result))),
      children
    }
  }

  fn execute_error(
    &mut self,
    _action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    path: &Vec<String>
  ) -> ExecutionPlanNode {
    let (children, str_values) = match self.evaluate_children(value_resolver, node, path) {
      Ok((children, values)) => {
        (children, values.iter().flat_map(|v| {
          match v {
            NodeValue::STRING(s) => vec![s.clone()],
            NodeValue::BOOL(b) => vec![b.to_string()],
            NodeValue::MMAP(_) => vec![v.str_form()],
            NodeValue::SLIST(list) => list.clone(),
            NodeValue::BARRAY(_) => vec![v.str_form()],
            NodeValue::NAMESPACED(_, _) => vec![v.str_form()],
            NodeValue::UINT(u) => vec![u.to_string()],
            NodeValue::JSON(json) => vec![json.to_string()],
            _ => vec![]
          }
        }).collect_vec())
      },
      Err(value) => return value
    };

    let result = str_values.iter().join("");
    ExecutionPlanNode {
      node_type: node.node_type.clone(),
      result: Some(NodeResult::ERROR(result)),
      children
    }
  }

  fn evaluate_children(
    &mut self,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    path: &Vec<String>
  ) -> Result<(Vec<ExecutionPlanNode>, Vec<NodeValue>), ExecutionPlanNode> {
    let mut children = vec![];
    let mut values = vec![];
    let mut loop_items = VecDeque::from(node.children.clone());

    while !loop_items.is_empty() {
      let child = loop_items.pop_front().unwrap();
      let value = if let Some(child_value) = child.value() {
        child_value
      } else {
        match &walk_tree(path.as_slice(), &child, value_resolver, self) {
          Ok(value) => if value.is_splat() {
            children.push(value.clone());
            for splat_child in value.children.iter().rev() {
              loop_items.push_front(splat_child.clone());
            }
            NodeResult::OK
          } else {
            children.push(value.clone());
            value.value().unwrap_or_default()
          },
          Err(err) => {
            return Err(ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::ERROR(err.to_string())),
              children: children.clone()
            })
          }
        }
      };

      match value {
        NodeResult::OK => {
          // no-op
        }
        NodeResult::VALUE(value) => {
          values.push(value);
        }
        NodeResult::ERROR(err) => {
          return Err(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR(err.to_string())),
            children: children.clone()
          })
        }
      }
    }
    Ok((children, values))
  }

  fn execute_check_entries(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    match self.validate_args(2, 1, node, action, value_resolver, &action_path) {
      Ok((values, optional)) => {
        let first = values[0].value()
          .unwrap_or_default()
          .as_value()
          .unwrap_or_default()
          .as_slist()
          .unwrap_or_default();
        let expected_keys = first.iter()
          .cloned()
          .collect::<HashSet<_>>();
        let second = values[1].value()
          .unwrap_or_default()
          .as_value()
          .unwrap_or_default();
        let result = match &second {
          NodeValue::MMAP(map) => {
            let actual_keys = map.keys()
              .cloned()
              .collect::<HashSet<_>>();
            match action {
              "expect:entries" => {
                let diff = &expected_keys - &actual_keys;
                if diff.is_empty() {
                  Ok(())
                } else {
                  let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                  Err((format!("The following expected entries were missing: {}", keys), Some(diff)))
                }
              }
              "expect:only-entries" => {
                let diff = &actual_keys - &expected_keys;
                if diff.is_empty() {
                  Ok(())
                } else {
                  let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                  Err((format!("The following unexpected entries were received: {}", keys), Some(diff)))
                }
              }
              _ => Err((format!("'{}' is not a valid action", action), None))
            }
          }
          NodeValue::SLIST(list) => {
            let actual_keys = list.iter()
              .cloned()
              .collect::<HashSet<_>>();
            match action {
              "expect:entries" => {
                let diff = &expected_keys - &actual_keys;
                if diff.is_empty() {
                  Ok(())
                } else {
                  let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                  Err((format!("The following expected entries were missing: {}", keys), Some(diff)))
                }
              }
              "expect:only-entries" => {
                let diff = &actual_keys - &expected_keys;
                if diff.is_empty() {
                  Ok(())
                } else {
                  let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                  Err((format!("The following unexpected entries were received: {}", keys), Some(diff)))
                }
              }
              _ => Err((format!("'{}' is not a valid action", action), None))
            }
          }
          NodeValue::JSON(json) => match json {
            Value::Object(map) => {
              let actual_keys = map.keys()
                .cloned()
                .collect::<HashSet<_>>();
              match action {
                "expect:entries" => {
                  let diff = &expected_keys - &actual_keys;
                  if diff.is_empty() {
                    Ok(())
                  } else {
                    let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                    Err((format!("The following expected entries were missing: {}", keys), Some(diff)))
                  }
                }
                "expect:only-entries" => {
                  let diff = &actual_keys - &expected_keys;
                  if diff.is_empty() {
                    Ok(())
                  } else {
                    let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                    Err((format!("The following unexpected entries were received: {}", keys), Some(diff)))
                  }
                }
                _ => Err((format!("'{}' is not a valid action", action), None))
              }
            }
            Value::Array(list) => {
              let actual_keys = list.iter()
                .map(|v| v.to_string())
                .collect::<HashSet<_>>();
              match action {
                "expect:entries" => {
                  let diff = &expected_keys - &actual_keys;
                  if diff.is_empty() {
                    Ok(())
                  } else {
                    let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                    Err((format!("The following expected entries were missing: {}", keys), Some(diff)))
                  }
                }
                "expect:only-entries" => {
                  let diff = &actual_keys - &expected_keys;
                  if diff.is_empty() {
                    Ok(())
                  } else {
                    let keys = NodeValue::SLIST(diff.iter().cloned().collect_vec());
                    Err((format!("The following unexpected entries were received: {}", keys), Some(diff)))
                  }
                }
                _ => Err((format!("'{}' is not a valid action", action), None))
              }
            }
            _ => Err((format!("'{}' can't be used with a {:?} node", action, second), None))
          }
          _ => Err((format!("'{}' can't be used with a {:?} node", action, second), None))
        };

        match result {
          Ok(_) => {
            ExecutionPlanNode {
              node_type: node.node_type.clone(),
              result: Some(NodeResult::OK),
              children: values.iter().chain(optional.iter()).cloned().collect()
            }
          }
          Err((err, diff)) => {
            debug!("expect:empty failed with an error: {}", err);
            if optional.len() > 0 {
              if let Some(diff) = diff {
                self.push_result(Some(NodeResult::VALUE(NodeValue::SLIST(diff.iter().cloned().collect()))));
                let result = if let Ok(value) = walk_tree(action_path.as_slice(), &optional[0], value_resolver, self) {
                  let message = value.value().unwrap_or_default().as_string().unwrap_or_default();
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(NodeResult::ERROR(message)),
                    children: values.iter().chain(once(&value)).cloned().collect()
                  }
                } else {
                  // There was an error generating the optional message, so just return the
                  // original error
                  ExecutionPlanNode {
                    node_type: node.node_type.clone(),
                    result: Some(NodeResult::ERROR(err.to_string())),
                    children: values.iter().chain(optional.iter()).cloned().collect()
                  }
                };
                self.pop_result();
                result
              } else {
                ExecutionPlanNode {
                  node_type: node.node_type.clone(),
                  result: Some(NodeResult::ERROR(err.to_string())),
                  children: values.iter().chain(optional.iter()).cloned().collect()
                }
              }
            } else {
              ExecutionPlanNode {
                node_type: node.node_type.clone(),
                result: Some(NodeResult::ERROR(err.to_string())),
                children: values.iter().chain(optional.iter()).cloned().collect()
              }
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

  fn execute_header_parse(
    &mut self,
    action: &str,
    value_resolver: &dyn ValueResolver,
    node: &ExecutionPlanNode,
    action_path: &Vec<String>
  ) -> ExecutionPlanNode {
    match self.validate_one_arg(node, action, value_resolver, &action_path) {
      Ok(value) => {
        let arg_value = value.value()
          .unwrap_or_default()
          .as_string()
          .unwrap_or_default();
        let values: Vec<&str> = strip_whitespace(arg_value.as_str(), ";");
        let (header_value, header_params) = values.as_slice()
          .split_first()
          .unwrap_or((&"", &[]));
        let parameter_map = parse_charset_parameters(header_params);

        ExecutionPlanNode {
          node_type: node.node_type.clone(),
          result: Some(NodeResult::VALUE(NodeValue::JSON(json!({
            "value": header_value,
            "parameters": parameter_map
          })))),
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
