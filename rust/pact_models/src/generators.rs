//! `generators` module includes all the classes to deal with V3/V4 spec generators

#[cfg(test)] use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};
use std::mem;
use std::str::FromStr;

use chrono::Local;
#[cfg(test)] use expectest::prelude::*;
use itertools::Itertools;
use log::*;
use maplit::hashmap;
use rand::distributions::Alphanumeric;
use rand::prelude::*;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::bodies::OptionalBody;
use crate::expression_parser::{contains_expressions, DataType, DataValue, MapValueResolver, parse_expression};
use crate::json_utils::{get_field_as_string, json_to_string, JsonToNum};
use crate::matchingrules::{Category, MatchingRuleCategory};
use crate::PactSpecification;
use crate::time_utils::{parse_pattern, to_chrono_pattern};

/// Trait to represent a generator
#[derive(Serialize, Deserialize, Debug, Clone, Eq)]
pub enum Generator {
  /// Generates a random integer between the min and max values
  RandomInt(i32, i32),
  /// Generates a random UUID value
  Uuid,
  /// Generates a random sequence of digits
  RandomDecimal(u16),
  /// Generates a random sequence of hexadecimal digits
  RandomHexadecimal(u16),
  /// Generates a random string of the provided size
  RandomString(u16),
  /// Generates a random string that matches the provided regex
  Regex(String),
  /// Generates a random date that matches either the provided format or the ISO format
  Date(Option<String>),
  /// Generates a random time that matches either the provided format or the ISO format
  Time(Option<String>),
  /// Generates a random timestamp that matches either the provided format or the ISO format
  DateTime(Option<String>),
  /// Generates a random boolean value
  RandomBoolean,
  /// Generates a value that is looked up from the provider state context
  ProviderStateGenerator(String, Option<DataType>),
  /// Generates a URL with the mock server as the base URL
  MockServerURL(String, String),
  /// List of variants which can have embedded generators
  ArrayContains(Vec<(usize, MatchingRuleCategory, HashMap<String, Generator>)>)
}

impl Generator {
  /// Convert this generator to a JSON struct
  pub fn to_json(&self) -> Option<Value> {
    match self {
      Generator::RandomInt(min, max) => Some(json!({ "type": "RandomInt", "min": min, "max": max })),
      Generator::Uuid => Some(json!({ "type": "Uuid" })),
      Generator::RandomDecimal(digits) => Some(json!({ "type": "RandomDecimal", "digits": digits })),
      Generator::RandomHexadecimal(digits) => Some(json!({ "type": "RandomHexadecimal", "digits": digits })),
      Generator::RandomString(size) => Some(json!({ "type": "RandomString", "size": size })),
      Generator::Regex(ref regex) => Some(json!({ "type": "Regex", "regex": regex })),
      Generator::Date(ref format) => match format {
        Some(ref format) => Some(json!({ "type": "Date", "format": format })),
        None => Some(json!({ "type": "Date" }))
      },
      Generator::Time(ref format) => match format {
        Some(ref format) => Some(json!({ "type": "Time", "format": format })),
        None => Some(json!({ "type": "Time" }))
      },
      Generator::DateTime(ref format) => match format {
        Some(ref format) => Some(json!({ "type": "DateTime", "format": format })),
        None => Some(json!({ "type": "DateTime" }))
      },
      Generator::RandomBoolean => Some(json!({ "type": "RandomBoolean" })),
      Generator::ProviderStateGenerator(ref expression, ref data_type) => {
        if let Some(data_type) = data_type {
          Some(json!({"type": "ProviderState", "expression": expression, "dataType": data_type}))
        } else {
          Some(json!({"type": "ProviderState", "expression": expression}))
        }
      }
      Generator::MockServerURL(example, regex) => Some(json!({ "type": "MockServerURL", "example": example, "regex": regex })),
      _ => None
    }
  }

  /// Converts a JSON map into a `Generator` struct, returning `None` if it can not be converted.
  pub fn from_map(gen_type: &str, map: &serde_json::Map<String, Value>) -> Option<Generator> {
    match gen_type {
      "RandomInt" => {
        let min = <i32>::json_to_number(map, "min", 0);
        let max = <i32>::json_to_number(map, "max", 10);
        Some(Generator::RandomInt(min, max))
      },
      "Uuid" => Some(Generator::Uuid),
      "RandomDecimal" => Some(Generator::RandomDecimal(<u16>::json_to_number(map, "digits", 10))),
      "RandomHexadecimal" => Some(Generator::RandomHexadecimal(<u16>::json_to_number(map, "digits", 10))),
      "RandomString" => Some(Generator::RandomString(<u16>::json_to_number(map, "size", 10))),
      "Regex" => map.get("regex").map(|val| Generator::Regex(json_to_string(val))),
      "Date" => Some(Generator::Date(get_field_as_string("format", map))),
      "Time" => Some(Generator::Time(get_field_as_string("format", map))),
      "DateTime" => Some(Generator::DateTime(get_field_as_string("format", map))),
      "RandomBoolean" => Some(Generator::RandomBoolean),
      "ProviderState" => map.get("expression").map(|f|
        Generator::ProviderStateGenerator(json_to_string(f), map.get("dataType")
          .map(|dt| DataType::from(dt.clone())))),
      "MockServerURL" => Some(Generator::MockServerURL(get_field_as_string("example", map).unwrap_or_default(),
                                                       get_field_as_string("regex", map).unwrap_or_default())),
      _ => {
        log::warn!("'{}' is not a valid generator type", gen_type);
        None
      }
    }
  }

  /// If this generator is compatible with the given generator mode
  pub fn corresponds_to_mode(&self, mode: &GeneratorTestMode) -> bool {
    match self {
      Generator::ProviderStateGenerator(_, _) => mode == &GeneratorTestMode::Provider,
      Generator::MockServerURL(_, _) => mode == &GeneratorTestMode::Consumer,
      _ => true
    }
  }
}

impl Hash for Generator {
  fn hash<H: Hasher>(&self, state: &mut H) {
    mem::discriminant(self).hash(state);
    match self {
      Generator::RandomInt(min, max) => {
        min.hash(state);
        max.hash(state);
      },
      Generator::RandomDecimal(digits) => digits.hash(state),
      Generator::RandomHexadecimal(digits) => digits.hash(state),
      Generator::RandomString(size) => size.hash(state),
      Generator::Regex(re) => re.hash(state),
      Generator::DateTime(format) => format.hash(state),
      Generator::Time(format) => format.hash(state),
      Generator::Date(format) => format.hash(state),
      Generator::ProviderStateGenerator(str, datatype) => {
        str.hash(state);
        datatype.hash(state);
      },
      Generator::MockServerURL(str1, str2) => {
        str1.hash(state);
        str2.hash(state);
      },
      Generator::ArrayContains(variants) => {
        for (index, rules, generators) in variants {
          index.hash(state);
          rules.hash(state);
          for (s, g) in generators {
            s.hash(state);
            g.hash(state);
          }
        }
      }
      _ => ()
    }
  }
}

impl PartialEq for Generator {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Generator::RandomInt(min1, max1), Generator::RandomInt(min2, max2)) => min1 == min2 && max1 == max2,
      (Generator::RandomDecimal(digits1), Generator::RandomDecimal(digits2)) => digits1 == digits2,
      (Generator::RandomHexadecimal(digits1), Generator::RandomHexadecimal(digits2)) => digits1 == digits2,
      (Generator::RandomString(size1), Generator::RandomString(size2)) => size1 == size2,
      (Generator::Regex(re1), Generator::Regex(re2)) => re1 == re2,
      (Generator::DateTime(format1), Generator::DateTime(format2)) => format1 == format2,
      (Generator::Time(format1), Generator::Time(format2)) => format1 == format2,
      (Generator::Date(format1), Generator::Date(format2)) => format1 == format2,
      (Generator::ProviderStateGenerator(str1, data1), Generator::ProviderStateGenerator(str2, data2)) => str1 == str2 && data1 == data2,
      (Generator::MockServerURL(ex1, re1), Generator::MockServerURL(ex2, re2)) => ex1 == ex2 && re1 == re2,
      (Generator::ArrayContains(variants1), Generator::ArrayContains(variants2)) => variants1 == variants2,
      _ => mem::discriminant(self) == mem::discriminant(other)
    }
  }
}

#[cfg(test)]
fn h(rule: &Generator) -> u64 {
  let mut hasher = DefaultHasher::new();
  rule.hash(&mut hasher);
  hasher.finish()
}

#[test]
fn hash_and_partial_eq_for_matching_rule() {
  expect!(h(&Generator::Uuid)).to(be_equal_to(h(&Generator::Uuid)));
  expect!(Generator::Uuid).to(be_equal_to(Generator::Uuid));
  expect!(Generator::Uuid).to_not(be_equal_to(Generator::RandomBoolean));

  expect!(h(&Generator::RandomBoolean)).to(be_equal_to(h(&Generator::RandomBoolean)));
  expect!(Generator::RandomBoolean).to(be_equal_to(Generator::RandomBoolean));

  let randint1 = Generator::RandomInt(100, 200);
  let randint2 = Generator::RandomInt(200, 200);

  expect!(h(&randint1)).to(be_equal_to(h(&randint1)));
  expect!(&randint1).to(be_equal_to(&randint1));
  expect!(h(&randint1)).to_not(be_equal_to(h(&randint2)));
  expect!(&randint1).to_not(be_equal_to(&randint2));

  let dec1 = Generator::RandomDecimal(100);
  let dec2 = Generator::RandomDecimal(200);

  expect!(h(&dec1)).to(be_equal_to(h(&dec1)));
  expect!(&dec1).to(be_equal_to(&dec1));
  expect!(h(&dec1)).to_not(be_equal_to(h(&dec2)));
  expect!(&dec1).to_not(be_equal_to(&dec2));

  let hexdec1 = Generator::RandomHexadecimal(100);
  let hexdec2 = Generator::RandomHexadecimal(200);

  expect!(h(&hexdec1)).to(be_equal_to(h(&hexdec1)));
  expect!(&hexdec1).to(be_equal_to(&hexdec1));
  expect!(h(&hexdec1)).to_not(be_equal_to(h(&hexdec2)));
  expect!(&hexdec1).to_not(be_equal_to(&hexdec2));

  let str1 = Generator::RandomString(100);
  let str2 = Generator::RandomString(200);

  expect!(h(&str1)).to(be_equal_to(h(&str1)));
  expect!(&str1).to(be_equal_to(&str1));
  expect!(h(&str1)).to_not(be_equal_to(h(&str2)));
  expect!(&str1).to_not(be_equal_to(&str2));

  let regex1 = Generator::Regex("\\d+".into());
  let regex2 = Generator::Regex("\\w+".into());

  expect!(h(&regex1)).to(be_equal_to(h(&regex1)));
  expect!(&regex1).to(be_equal_to(&regex1));
  expect!(h(&regex1)).to_not(be_equal_to(h(&regex2)));
  expect!(&regex1).to_not(be_equal_to(&regex2));

  let datetime1 = Generator::DateTime(Some("yyyy-MM-dd HH:mm:ss".into()));
  let datetime2 = Generator::DateTime(Some("yyyy-MM-ddTHH:mm:ss".into()));

  expect!(h(&datetime1)).to(be_equal_to(h(&datetime1)));
  expect!(&datetime1).to(be_equal_to(&datetime1));
  expect!(h(&datetime1)).to_not(be_equal_to(h(&datetime2)));
  expect!(&datetime1).to_not(be_equal_to(&datetime2));

  let date1 = Generator::Date(Some("yyyy-MM-dd".into()));
  let date2 = Generator::Date(Some("yy-MM-dd".into()));

  expect!(h(&date1)).to(be_equal_to(h(&date1)));
  expect!(&date1).to(be_equal_to(&date1));
  expect!(h(&date1)).to_not(be_equal_to(h(&date2)));
  expect!(&date1).to_not(be_equal_to(&date2));

  let time1 = Generator::Time(Some("HH:mm:ss".into()));
  let time2 = Generator::Time(Some("hh:mm:ss".into()));

  expect!(h(&time1)).to(be_equal_to(h(&time1)));
  expect!(&time1).to(be_equal_to(&time1));
  expect!(h(&time1)).to_not(be_equal_to(h(&time2)));
  expect!(&time1).to_not(be_equal_to(&time2));

  let psg1 = Generator::ProviderStateGenerator("string one".into(), Some(DataType::BOOLEAN));
  let psg2 = Generator::ProviderStateGenerator("string two".into(), None);
  let psg3 = Generator::ProviderStateGenerator("string one".into(), None);

  expect!(h(&psg1)).to(be_equal_to(h(&psg1)));
  expect!(&psg1).to(be_equal_to(&psg1));
  expect!(h(&psg1)).to_not(be_equal_to(h(&psg2)));
  expect!(h(&psg1)).to_not(be_equal_to(h(&psg3)));
  expect!(&psg1).to_not(be_equal_to(&psg2));
  expect!(&psg1).to_not(be_equal_to(&psg3));

  let msu1 = Generator::MockServerURL("string one".into(), "\\d+".into());
  let msu2 = Generator::MockServerURL("string two".into(), "\\d+".into());
  let msu3 = Generator::MockServerURL("string one".into(), "\\w+".into());

  expect!(h(&msu1)).to(be_equal_to(h(&msu1)));
  expect!(&msu1).to(be_equal_to(&msu1));
  expect!(h(&msu1)).to_not(be_equal_to(h(&msu2)));
  expect!(h(&msu1)).to_not(be_equal_to(h(&msu3)));
  expect!(&msu1).to_not(be_equal_to(&msu2));
  expect!(&msu1).to_not(be_equal_to(&msu3));

  let ac1 = Generator::ArrayContains(vec![]);
  let ac2 = Generator::ArrayContains(vec![(0, MatchingRuleCategory::empty("body"), hashmap!{})]);
  let ac3 = Generator::ArrayContains(vec![(1, MatchingRuleCategory::empty("body"), hashmap!{})]);
  let ac4 = Generator::ArrayContains(vec![(0, MatchingRuleCategory::equality("body"), hashmap!{})]);
  let ac5 = Generator::ArrayContains(vec![(0, MatchingRuleCategory::empty("body"), hashmap!{ "A".to_string() => Generator::RandomBoolean })]);
  let ac6 = Generator::ArrayContains(vec![
    (0, MatchingRuleCategory::empty("body"), hashmap!{ "A".to_string() => Generator::RandomBoolean }),
    (1, MatchingRuleCategory::empty("body"), hashmap!{ "A".to_string() => Generator::RandomDecimal(10) })
  ]);
  let ac7 = Generator::ArrayContains(vec![
    (0, MatchingRuleCategory::empty("body"), hashmap!{ "A".to_string() => Generator::RandomBoolean }),
    (1, MatchingRuleCategory::equality("body"), hashmap!{ "A".to_string() => Generator::RandomDecimal(10) })
  ]);

  expect!(h(&ac1)).to(be_equal_to(h(&ac1)));
  expect!(h(&ac1)).to_not(be_equal_to(h(&ac2)));
  expect!(h(&ac1)).to_not(be_equal_to(h(&ac3)));
  expect!(h(&ac1)).to_not(be_equal_to(h(&ac4)));
  expect!(h(&ac1)).to_not(be_equal_to(h(&ac5)));
  expect!(h(&ac1)).to_not(be_equal_to(h(&ac6)));
  expect!(h(&ac1)).to_not(be_equal_to(h(&ac7)));
  expect!(h(&ac2)).to(be_equal_to(h(&ac2)));
  expect!(h(&ac2)).to_not(be_equal_to(h(&ac1)));
  expect!(h(&ac2)).to_not(be_equal_to(h(&ac3)));
  expect!(h(&ac2)).to_not(be_equal_to(h(&ac4)));
  expect!(h(&ac2)).to_not(be_equal_to(h(&ac5)));
  expect!(h(&ac2)).to_not(be_equal_to(h(&ac6)));
  expect!(h(&ac2)).to_not(be_equal_to(h(&ac7)));
  expect!(h(&ac3)).to(be_equal_to(h(&ac3)));
  expect!(h(&ac3)).to_not(be_equal_to(h(&ac2)));
  expect!(h(&ac3)).to_not(be_equal_to(h(&ac1)));
  expect!(h(&ac3)).to_not(be_equal_to(h(&ac4)));
  expect!(h(&ac3)).to_not(be_equal_to(h(&ac5)));
  expect!(h(&ac3)).to_not(be_equal_to(h(&ac6)));
  expect!(h(&ac3)).to_not(be_equal_to(h(&ac7)));
  expect!(h(&ac4)).to(be_equal_to(h(&ac4)));
  expect!(h(&ac4)).to_not(be_equal_to(h(&ac2)));
  expect!(h(&ac4)).to_not(be_equal_to(h(&ac3)));
  expect!(h(&ac4)).to_not(be_equal_to(h(&ac1)));
  expect!(h(&ac4)).to_not(be_equal_to(h(&ac5)));
  expect!(h(&ac4)).to_not(be_equal_to(h(&ac6)));
  expect!(h(&ac4)).to_not(be_equal_to(h(&ac7)));
  expect!(h(&ac5)).to(be_equal_to(h(&ac5)));
  expect!(h(&ac5)).to_not(be_equal_to(h(&ac2)));
  expect!(h(&ac5)).to_not(be_equal_to(h(&ac3)));
  expect!(h(&ac5)).to_not(be_equal_to(h(&ac4)));
  expect!(h(&ac5)).to_not(be_equal_to(h(&ac1)));
  expect!(h(&ac5)).to_not(be_equal_to(h(&ac6)));
  expect!(h(&ac5)).to_not(be_equal_to(h(&ac7)));
  expect!(h(&ac6)).to(be_equal_to(h(&ac6)));
  expect!(h(&ac6)).to_not(be_equal_to(h(&ac2)));
  expect!(h(&ac6)).to_not(be_equal_to(h(&ac3)));
  expect!(h(&ac6)).to_not(be_equal_to(h(&ac4)));
  expect!(h(&ac6)).to_not(be_equal_to(h(&ac5)));
  expect!(h(&ac6)).to_not(be_equal_to(h(&ac1)));
  expect!(h(&ac6)).to_not(be_equal_to(h(&ac7)));
  expect!(h(&ac7)).to(be_equal_to(h(&ac7)));
  expect!(h(&ac7)).to_not(be_equal_to(h(&ac2)));
  expect!(h(&ac7)).to_not(be_equal_to(h(&ac3)));
  expect!(h(&ac7)).to_not(be_equal_to(h(&ac4)));
  expect!(h(&ac7)).to_not(be_equal_to(h(&ac5)));
  expect!(h(&ac7)).to_not(be_equal_to(h(&ac6)));
  expect!(h(&ac7)).to_not(be_equal_to(h(&ac1)));

  expect!(&ac1).to(be_equal_to(&ac1));
  expect!(&ac1).to_not(be_equal_to(&ac2));
  expect!(&ac1).to_not(be_equal_to(&ac3));
  expect!(&ac1).to_not(be_equal_to(&ac4));
  expect!(&ac1).to_not(be_equal_to(&ac5));
  expect!(&ac1).to_not(be_equal_to(&ac6));
  expect!(&ac1).to_not(be_equal_to(&ac7));
  expect!(&ac2).to(be_equal_to(&ac2));
  expect!(&ac2).to_not(be_equal_to(&ac1));
  expect!(&ac2).to_not(be_equal_to(&ac3));
  expect!(&ac2).to_not(be_equal_to(&ac4));
  expect!(&ac2).to_not(be_equal_to(&ac5));
  expect!(&ac2).to_not(be_equal_to(&ac6));
  expect!(&ac2).to_not(be_equal_to(&ac7));
  expect!(&ac3).to(be_equal_to(&ac3));
  expect!(&ac3).to_not(be_equal_to(&ac2));
  expect!(&ac3).to_not(be_equal_to(&ac1));
  expect!(&ac3).to_not(be_equal_to(&ac4));
  expect!(&ac3).to_not(be_equal_to(&ac5));
  expect!(&ac3).to_not(be_equal_to(&ac6));
  expect!(&ac3).to_not(be_equal_to(&ac7));
  expect!(&ac4).to(be_equal_to(&ac4));
  expect!(&ac4).to_not(be_equal_to(&ac2));
  expect!(&ac4).to_not(be_equal_to(&ac3));
  expect!(&ac4).to_not(be_equal_to(&ac1));
  expect!(&ac4).to_not(be_equal_to(&ac5));
  expect!(&ac4).to_not(be_equal_to(&ac6));
  expect!(&ac4).to_not(be_equal_to(&ac7));
  expect!(&ac5).to(be_equal_to(&ac5));
  expect!(&ac5).to_not(be_equal_to(&ac2));
  expect!(&ac5).to_not(be_equal_to(&ac3));
  expect!(&ac5).to_not(be_equal_to(&ac4));
  expect!(&ac5).to_not(be_equal_to(&ac1));
  expect!(&ac5).to_not(be_equal_to(&ac6));
  expect!(&ac5).to_not(be_equal_to(&ac7));
  expect!(&ac6).to(be_equal_to(&ac6));
  expect!(&ac6).to_not(be_equal_to(&ac2));
  expect!(&ac6).to_not(be_equal_to(&ac3));
  expect!(&ac6).to_not(be_equal_to(&ac4));
  expect!(&ac6).to_not(be_equal_to(&ac5));
  expect!(&ac6).to_not(be_equal_to(&ac1));
  expect!(&ac6).to_not(be_equal_to(&ac7));
  expect!(&ac7).to(be_equal_to(&ac7));
  expect!(&ac7).to_not(be_equal_to(&ac2));
  expect!(&ac7).to_not(be_equal_to(&ac3));
  expect!(&ac7).to_not(be_equal_to(&ac4));
  expect!(&ac7).to_not(be_equal_to(&ac5));
  expect!(&ac7).to_not(be_equal_to(&ac6));
  expect!(&ac7).to_not(be_equal_to(&ac1));
}


/// If the generators are being applied in the context of a consumer or provider
#[derive(Debug, Clone, PartialEq)]
pub enum GeneratorTestMode {
  /// Generate values in the context of the consumer
  Consumer,
  /// Generate values in the context of the provider
  Provider
}


/// Category that the generator is applied to
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Eq, Hash)]
pub enum GeneratorCategory {
  /// Request Method
  METHOD,
  /// Request Path
  PATH,
  /// Request/Response Header
  HEADER,
  /// Request Query Parameter
  QUERY,
  /// Body
  BODY,
  /// Response Status
  STATUS
}

impl FromStr for GeneratorCategory {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "method" => Ok(GeneratorCategory::METHOD),
      "path" => Ok(GeneratorCategory::PATH),
      "header" => Ok(GeneratorCategory::HEADER),
      "query" => Ok(GeneratorCategory::QUERY),
      "body" => Ok(GeneratorCategory::BODY),
      "status" => Ok(GeneratorCategory::STATUS),
      _ => Err(format!("'{}' is not a valid GeneratorCategory", s))
    }
  }
}

impl <'a> Into<&'a str> for GeneratorCategory {
  fn into(self) -> &'a str {
    match self {
      GeneratorCategory::METHOD => "method",
      GeneratorCategory::PATH => "path",
      GeneratorCategory::HEADER => "header",
      GeneratorCategory::QUERY => "query",
      GeneratorCategory::BODY => "body",
      GeneratorCategory::STATUS => "status"
    }
  }
}

impl Into<String> for GeneratorCategory {
  fn into(self) -> String {
    let s: &str = self.into();
    s.to_string()
  }
}

impl Into<Category> for GeneratorCategory {
  fn into(self) -> Category {
    match self {
      GeneratorCategory::METHOD => Category::METHOD,
      GeneratorCategory::PATH => Category::PATH,
      GeneratorCategory::HEADER => Category::HEADER,
      GeneratorCategory::QUERY => Category::QUERY,
      GeneratorCategory::BODY => Category::BODY,
      GeneratorCategory::STATUS => Category::STATUS
    }
  }
}


/// Trait for something that can generate a value based on a source value.
pub trait GenerateValue<T> {
  /// Generates a new value based on the source value. An error will be returned if the value can not
  /// be generated.
  fn generate_value(&self, value: &T, context: &HashMap<&str, Value>) -> Result<T, String>;
}

/// Trait to define a handler for applying generators to data of a particular content type.
pub trait ContentTypeHandler<T> {
  /// Processes the body using the map of generators, returning a (possibly) updated body.
  fn process_body(&mut self, generators: &HashMap<String, Generator>, mode: &GeneratorTestMode, context: &HashMap<&str, Value>) -> Result<OptionalBody, String>;
  /// Applies the generator to the key in the body.
  fn apply_key(&mut self, key: &String, generator: &dyn GenerateValue<T>, context: &HashMap<&str, Value>);
}

/// Data structure for representing a collection of generators
#[derive(Serialize, Deserialize, Debug, Clone, Eq)]
#[serde(transparent)]
pub struct Generators {
  /// Map of generator categories to maps of generators
  pub categories: HashMap<GeneratorCategory, HashMap<String, Generator>>
}

impl Generators {
  /// If the generators are empty (that is there are no rules assigned to any categories)
  pub fn is_empty(&self) -> bool {
    self.categories.values().all(|category| category.is_empty())
  }

  /// If the generators are not empty (that is there is at least one rule assigned to a category)
  pub fn is_not_empty(&self) -> bool {
    self.categories.values().any(|category| !category.is_empty())
  }

  /// Loads the generators for a JSON map
  pub fn load_from_map(&mut self, map: &serde_json::Map<String, Value>) {
    for (k, v) in map {
      match v {
        &Value::Object(ref map) =>  match GeneratorCategory::from_str(k) {
          Ok(ref category) => match category {
            &GeneratorCategory::PATH | &GeneratorCategory::METHOD | &GeneratorCategory::STATUS => {
              self.parse_generator_from_map(category, map, None);
            },
            _ => for (sub_k, sub_v) in map {
              match sub_v {
                &Value::Object(ref map) => self.parse_generator_from_map(category, map, Some(sub_k.clone())),
                _ => log::warn!("Ignoring invalid generator JSON '{}' -> {:?}", sub_k, sub_v)
              }
            }
          },
          Err(err) => log::warn!("Ignoring generator with invalid category '{}' - {}", k, err)
        },
        _ => log::warn!("Ignoring invalid generator JSON '{}' -> {:?}", k, v)
      }
    }
  }

  pub(crate) fn parse_generator_from_map(&mut self, category: &GeneratorCategory,
                                         map: &serde_json::Map<String, Value>, subcat: Option<String>) {
    match map.get("type") {
      Some(gen_type) => match gen_type {
        &Value::String(ref gen_type) => match Generator::from_map(gen_type, map) {
          Some(generator) => match subcat {
            Some(s) => self.add_generator_with_subcategory(category, s, generator),
            None => self.add_generator(category, generator)
          },
          None => log::warn!("Ignoring invalid generator JSON '{:?}' with invalid type attribute -> {:?}", category, map)
        },
        _ => log::warn!("Ignoring invalid generator JSON '{:?}' with invalid type attribute -> {:?}", category, map)
      },
      None => log::warn!("Ignoring invalid generator JSON '{:?}' with no type attribute -> {:?}", category, map)
    }
  }

  fn to_json(&self) -> Value {
    Value::Object(self.categories.iter().fold(serde_json::Map::new(), |mut map, (name, category)| {
      let cat: String = name.clone().into();
      match name {
        &GeneratorCategory::PATH | &GeneratorCategory::METHOD | &GeneratorCategory::STATUS => {
          match category.get("") {
            Some(generator) => {
              let json = generator.to_json();
              if let Some(json) = json {
                map.insert(cat.clone(), json);
              }
            },
            None => ()
          }
        },
        _ => {
          let mut generators = serde_json::Map::new();
          for (key, val) in category {
            let json = val.to_json();
            if let Some(json) = json {
              generators.insert(key.clone(), json);
            }
          }
          map.insert(cat.clone(), Value::Object(generators));
        }
      }
      map
    }))
  }

  /// Adds the generator to the category (body, headers, etc.)
  pub fn add_generator(&mut self, category: &GeneratorCategory, generator: Generator) {
    self.add_generator_with_subcategory(category, "", generator);
  }

  /// Adds a generator to the category with a sub-category key (i.e. headers or query parameters)
  pub fn add_generator_with_subcategory<S: Into<String>>(&mut self, category: &GeneratorCategory,
                                                         subcategory: S, generator: Generator) {
    let category_map = self.categories.entry(category.clone()).or_insert(HashMap::new());
    category_map.insert(subcategory.into(), generator.clone());
  }
}

impl Hash for Generators {
  fn hash<H: Hasher>(&self, state: &mut H) {
    for (k, v) in self.categories.iter() {
      k.hash(state);
      for (k2, v2) in v.iter() {
        k2.hash(state);
        v2.hash(state);
      }
    }
  }
}

impl PartialEq for Generators {
  fn eq(&self, other: &Self) -> bool {
    self.categories == other.categories
  }

  fn ne(&self, other: &Self) -> bool {
    self.categories != other.categories
  }
}

impl Default for Generators {
  fn default() -> Self {
    Generators {
      categories: hashmap!{}
    }
  }
}

/// If the mode applies, invoke the callback for each of the generators
pub fn apply_generators<F>(
  mode: &GeneratorTestMode,
  generators: &HashMap<String, Generator>,
  closure: &mut F
) where F: FnMut(&String, &Generator) {
  for (key, value) in generators {
    if value.corresponds_to_mode(mode) {
      closure(&key, &value)
    }
  }
}

/// Parses the generators from the Value structure
pub fn generators_from_json(value: &Value) -> Generators {
  let mut generators = Generators::default();
  match value {
    &Value::Object(ref m) => match m.get("generators") {
      Some(gen_val) => match gen_val {
        &Value::Object(ref m) => generators.load_from_map(m),
        _ => ()
      },
      None => ()
    },
    _ => ()
  }
  generators
}

/// Generates a Value structure for the provided generators
pub fn generators_to_json(generators: &Generators, spec_version: &PactSpecification) -> Value {
  match spec_version {
    &PactSpecification::V3 | &PactSpecification::V4 => generators.to_json(),
    _ => Value::Null
  }
}

/// Macro to make constructing generators easy
/// Example usage:
/// ```ignore
/// generators! {
///   "HEADER" => {
///     "A" => Generator::Uuid
///   }
/// }
///```
#[macro_export]
macro_rules! generators {
  (
    $( $category:expr => {
      $( $subname:expr => $generator:expr ), *
    }), *
  ) => {{
    let mut _generators = $crate::generators::Generators::default();

  $(
    {
      use std::str::FromStr;
      let _cat = $crate::generators::GeneratorCategory::from_str($category).unwrap();
      $(
        _generators.add_generator_with_subcategory(&_cat, $subname, $generator);
      )*
    }
  )*

    _generators
  }};

  (
    $( $category:expr => $generator:expr ), *
  ) => {{
    let mut _generators = $crate::generators::Generators::default();
    $(
      let _cat = $crate::generators::GeneratorCategory::from_str($category).unwrap();
      _generators.add_generator(&_cat, $generator);
    )*
    _generators
  }};
}

pub fn generate_value_from_context(expression: &str, context: &HashMap<&str, Value>, data_type: &Option<DataType>) -> Result<DataValue, String> {
  let result = if contains_expressions(expression) {
    parse_expression(expression, &MapValueResolver { context: context.clone() })
  } else {
    context.get(expression).map(|v| json_to_string(v))
      .ok_or(format!("Value '{}' was not found in the provided context", expression))
  };
  data_type.clone().unwrap_or(DataType::RAW).wrap(result)
}

const DIGIT_CHARSET: &str = "0123456789";
pub fn generate_decimal(digits: usize) -> String {
  let mut rnd = rand::thread_rng();
  let chars: Vec<char> = DIGIT_CHARSET.chars().collect();
  match digits {
    0 => "".to_string(),
    1 => chars.choose(&mut rnd).unwrap().to_string(),
    2 => format!("{}.{}", chars.choose(&mut rnd).unwrap(), chars.choose(&mut rnd).unwrap()),
    _ => {
      let mut sample = String::new();
      for _ in 0..(digits + 1) {
        sample.push(*chars.choose(&mut rnd).unwrap());
      }
      if sample.starts_with("00") {
        let chars = DIGIT_CHARSET[1..].chars();
        sample.insert(0, chars.choose(&mut rnd).unwrap());
      }
      let pos = rnd.gen_range(1..digits - 1);
      let selected_digits = if pos != 1 && sample.starts_with('0') {
        &sample[1..(digits + 1)]
      } else {
        &sample[..digits]
      };
      let generated = format!("{}.{}", &selected_digits[..pos], &selected_digits[pos..]);
      trace!("RandomDecimalGenerator: sample_digits=[{}], pos={}, selected_digits=[{}], generated=[{}]",
             sample, pos, selected_digits, generated);
      generated
    }
  }
}

const HEX_CHARSET: &str = "0123456789ABCDEF";
pub fn generate_hexadecimal(digits: usize) -> String {
  let mut rnd = rand::thread_rng();
  HEX_CHARSET.chars().choose_multiple(&mut rnd, digits).iter().join("")
}

impl GenerateValue<u16> for Generator {
  fn generate_value(&self, value: &u16, context: &HashMap<&str, Value>) -> Result<u16, String> {
    match self {
      &Generator::RandomInt(min, max) => Ok(rand::thread_rng().gen_range(min as u16..(max as u16).saturating_add(1))),
      &Generator::ProviderStateGenerator(ref exp, ref dt) =>
        match generate_value_from_context(exp, context, dt) {
          Ok(val) => u16::try_from(val),
          Err(err) => Err(err)
        },
      _ => Err(format!("Could not generate a u16 value from {} using {:?}", value, self))
    }
  }
}

pub fn generate_ascii_string(size: usize) -> String {
  rand::thread_rng().sample_iter(&Alphanumeric).map(char::from).take(size).collect()
}

fn strip_anchors(regex: &str) -> &str {
  regex
    .strip_prefix('^').unwrap_or(regex)
    .strip_suffix('$').unwrap_or(regex)
}

impl GenerateValue<String> for Generator {
  fn generate_value(&self, _: &String, context: &HashMap<&str, Value>) -> Result<String, String> {
    let mut rnd = rand::thread_rng();
    let result = match self {
      Generator::RandomInt(min, max) => Ok(format!("{}", rnd.gen_range(*min..max.saturating_add(1)))),
      Generator::Uuid => Ok(Uuid::new_v4().to_hyphenated().to_string()),
      Generator::RandomDecimal(digits) => Ok(generate_decimal(*digits as usize)),
      Generator::RandomHexadecimal(digits) => Ok(generate_hexadecimal(*digits as usize)),
      Generator::RandomString(size) => Ok(generate_ascii_string(*size as usize)),
      Generator::Regex(ref regex) => {
        let mut parser = regex_syntax::ParserBuilder::new().unicode(false).build();
        match parser.parse(strip_anchors(regex)) {
          Ok(hir) => {
            match rand_regex::Regex::with_hir(hir, 20) {
              Ok(gen) => Ok(rnd.sample(gen)),
              Err(err) => {
                log::warn!("Failed to generate a value from regular expression - {}", err);
                Err(format!("Failed to generate a value from regular expression - {}", err))
              }
            }
          },
          Err(err) => {
            log::warn!("'{}' is not a valid regular expression - {}", regex, err);
            Err(format!("'{}' is not a valid regular expression - {}", regex, err))
          }
        }
      },
      Generator::Date(ref format) => match format {
        Some(pattern) => match parse_pattern(pattern) {
          Ok(tokens) => Ok(Local::now().date().format(&to_chrono_pattern(&tokens)).to_string()),
          Err(err) => {
            log::warn!("Date format {} is not valid - {}", pattern, err);
            Err(format!("Date format {} is not valid - {}", pattern, err))
          }
        },
        None => Ok(Local::now().naive_local().date().to_string())
      },
      Generator::Time(ref format) => match format {
        Some(pattern) => match parse_pattern(pattern) {
          Ok(tokens) => Ok(Local::now().format(&to_chrono_pattern(&tokens)).to_string()),
          Err(err) => {
            log::warn!("Time format {} is not valid - {}", pattern, err);
            Err(format!("Time format {} is not valid - {}", pattern, err))
          }
        },
        None => Ok(Local::now().time().format("%H:%M:%S").to_string())
      },
      Generator::DateTime(ref format) => match format {
        Some(pattern) => match parse_pattern(pattern) {
          Ok(tokens) => Ok(Local::now().format(&to_chrono_pattern(&tokens)).to_string()),
          Err(err) => {
            log::warn!("DateTime format {} is not valid - {}", pattern, err);
            Err(format!("DateTime format {} is not valid - {}", pattern, err))
          }
        },
        None => Ok(Local::now().format("%Y-%m-%dT%H:%M:%S.%3f%z").to_string())
      },
      Generator::RandomBoolean => Ok(format!("{}", rnd.gen::<bool>())),
      Generator::ProviderStateGenerator(ref exp, ref dt) =>
        match generate_value_from_context(exp, context, dt) {
          Ok(val) => String::try_from(val),
          Err(err) => Err(err)
        },
      Generator::MockServerURL(example, regex) => if let Some(mock_server_details) = context.get("mockServer") {
        debug!("Generating URL from Mock Server details");
        match mock_server_details.as_object() {
          Some(mock_server_details) => {
            match get_field_as_string("url", mock_server_details) {
              Some(url) => match Regex::new(regex) {
                Ok(re) => Ok(re.replace(example, |caps: &Captures| {
                  format!("{}{}", url, caps.get(1).unwrap().as_str())
                }).to_string()),
                Err(err) => Err(format!("MockServerURL: Failed to generate value: {}", err))
              },
              None => Err("MockServerURL: can not generate a value as there is no mock server URL in the test context".to_string())
            }
          },
          None => Err("MockServerURL: can not generate a value as there is no mock server details in the test context".to_string())
        }
      } else {
        Err("MockServerURL: can not generate a value as there is no mock server details in the test context".to_string())
      },
      Generator::ArrayContains(_) => Err("can only use ArrayContains with lists".to_string())
    };
    debug!("Generator = {:?}, Generated value = {:?}", self, result);
    result
  }
}

impl GenerateValue<Vec<String>> for Generator {
  fn generate_value(&self, vals: &Vec<String>, context: &HashMap<&str, Value>) -> Result<Vec<String>, String> {
    self.generate_value(&vals.first().cloned().unwrap_or_default(), context).map(|v| vec![v])
  }
}

impl GenerateValue<Value> for Generator {
  fn generate_value(&self, value: &Value, context: &HashMap<&str, Value>) -> Result<Value, String> {
    debug!("Generating value from {:?} with context {:?}", self, context);
    let result = match self {
      Generator::RandomInt(min, max) => {
        let rand_int = rand::thread_rng().gen_range(*min..max.saturating_add(1));
        match value {
          Value::String(_) => Ok(json!(format!("{}", rand_int))),
          Value::Number(_) => Ok(json!(rand_int)),
          _ => Err(format!("Could not generate a random int from {}", value))
        }
      },
      Generator::Uuid => match value {
        Value::String(_) => Ok(json!(Uuid::new_v4().to_simple().to_string())),
        _ => Err(format!("Could not generate a UUID from {}", value))
      },
      Generator::RandomDecimal(digits) => match value {
        Value::String(_) => Ok(json!(generate_decimal(*digits as usize))),
        Value::Number(_) => match generate_decimal(*digits as usize).parse::<f64>() {
          Ok(val) => Ok(json!(val)),
          Err(err) => Err(format!("Could not generate a random decimal from {} - {}", value, err))
        },
        _ => Err(format!("Could not generate a random decimal from {}", value))
      },
      Generator::RandomHexadecimal(digits) => match value {
        Value::String(_) => Ok(json!(generate_hexadecimal(*digits as usize))),
        _ => Err(format!("Could not generate a random hexadecimal from {}", value))
      },
      Generator::RandomString(size) => match value {
        Value::String(_) => Ok(json!(generate_ascii_string(*size as usize))),
        _ => Err(format!("Could not generate a random string from {}", value))
      },
      Generator::Regex(ref regex) => {
        let mut parser = regex_syntax::ParserBuilder::new().unicode(false).build();
        match parser.parse(regex) {
          Ok(hir) => {
            let gen = rand_regex::Regex::with_hir(hir, 20).unwrap();
            Ok(json!(rand::thread_rng().sample::<String, _>(gen)))
          },
          Err(err) => {
            log::warn!("'{}' is not a valid regular expression - {}", regex, err);
            Err(format!("Could not generate a random string from {} - {}", regex, err))
          }
        }
      },
      Generator::Date(ref format) => match format {
        Some(pattern) => match parse_pattern(pattern) {
          Ok(tokens) => Ok(json!(Local::now().date().format(&to_chrono_pattern(&tokens)).to_string())),
          Err(err) => {
            log::warn!("Date format {} is not valid - {}", pattern, err);
            Err(format!("Could not generate a random date from {} - {}", pattern, err))
          }
        },
        None => Ok(json!(Local::now().naive_local().date().to_string()))
      },
      Generator::Time(ref format) => match format {
        Some(pattern) => match parse_pattern(pattern) {
          Ok(tokens) => Ok(json!(Local::now().format(&to_chrono_pattern(&tokens)).to_string())),
          Err(err) => {
            log::warn!("Time format {} is not valid - {}", pattern, err);
            Err(format!("Could not generate a random time from {} - {}", pattern, err))
          }
        },
        None => Ok(json!(Local::now().time().format("%H:%M:%S").to_string()))
      },
      Generator::DateTime(ref format) => match format {
        Some(pattern) => match parse_pattern(pattern) {
          Ok(tokens) => Ok(json!(Local::now().format(&to_chrono_pattern(&tokens)).to_string())),
          Err(err) => {
            log::warn!("DateTime format {} is not valid - {}", pattern, err);
            Err(format!("Could not generate a random date-time from {} - {}", pattern, err))
          }
        },
        None => Ok(json!(Local::now().format("%Y-%m-%dT%H:%M:%S.%3f%z").to_string()))
      },
      Generator::RandomBoolean => Ok(json!(rand::thread_rng().gen::<bool>())),
      Generator::ProviderStateGenerator(ref exp, ref dt) =>
        match generate_value_from_context(exp, context, dt) {
          Ok(val) => val.as_json(),
          Err(err) => Err(err)
        },
      Generator::MockServerURL(example, regex) => {
        debug!("context = {:?}", context);
        if let Some(mock_server_details) = context.get("mockServer") {
          match mock_server_details.as_object() {
            Some(mock_server_details) => {
              match get_field_as_string("href", mock_server_details) {
                Some(url) => match Regex::new(regex) {
                  Ok(re) => Ok(Value::String(re.replace(example, |caps: &Captures| {
                    format!("{}{}", url, caps.get(1).unwrap().as_str())
                  }).to_string())),
                  Err(err) => Err(format!("MockServerURL: Failed to generate value: {}", err))
                },
                None => Err("MockServerURL: can not generate a value as there is no mock server URL in the test context".to_string())
              }
            },
            None => Err("MockServerURL: can not generate a value as the mock server details in the test context is not an Object".to_string())
          }
        } else {
          Err("MockServerURL: can not generate a value as there is no mock server details in the test context".to_string())
        }
      }
      Generator::ArrayContains(variants) => match value {
        // TODO: this implementation needs values from pact matching crate
        // Value::Array(vec) => {
        //   let callback = |path: &Vec<&str>, value: &Value, context: &MatchingContext| {
        //     compare(path, value, value, context).is_ok()
        //   };
        //   let mut result = vec.clone();
        //   for (index, value) in vec.iter().enumerate() {
        //     if let Some((variant, generators)) = find_matching_variant(value, variants, &callback) {
        //       debug!("Generating values for variant {} and value {}", variant, value);
        //       let mut handler = JsonHandler { value: value.clone() };
        //       for (key, generator) in generators {
        //         handler.apply_key(&key, &generator, context);
        //       };
        //       debug!("Generated value {}", handler.value);
        //       result[index] = handler.value.clone();
        //     }
        //   }
        //   Ok(Value::Array(result))
        // }
        _ => Err("can only use ArrayContains with lists".to_string())
      }
    };
    debug!("Generated value = {:?}", result);
    result
  }
}

#[cfg(test)]
mod tests {
  use std::str::FromStr;

  use expectest::expect;
  use expectest::prelude::*;
  use hamcrest2::*;
  use test_env_log::test;

  use crate::generators::Generator::{RandomDecimal, RandomInt, Regex};

  use super::*;
  use super::Generator;

  #[test]
  fn rules_are_empty_when_there_are_no_categories() {
    expect!(Generators::default().is_empty()).to(be_true());
  }

  #[test]
  fn rules_are_empty_when_there_are_only_empty_categories() {
    expect!(Generators {
            categories: hashmap!{
                GeneratorCategory::BODY => hashmap!{},
                GeneratorCategory::HEADER => hashmap!{},
                GeneratorCategory::QUERY => hashmap!{}
            }
        }.is_empty()).to(be_true());
  }

  #[test]
  fn rules_are_not_empty_when_there_is_a_nonempty_category() {
    expect!(Generators {
            categories: hashmap!{
                GeneratorCategory::BODY => hashmap!{},
                GeneratorCategory::HEADER => hashmap!{},
                GeneratorCategory::QUERY => hashmap! {
                    "a".to_string() => Generator::RandomInt(1, 10)
                }
            }
        }.is_empty()).to(be_false());
  }

  #[test]
  fn matchers_from_json_test() {
    expect!(generators_from_json(&Value::Null).categories.iter()).to(be_empty());
  }

  #[test]
  fn generators_macro_test() {
    expect!(generators!{}).to(be_equal_to(Generators::default()));

    let mut expected = Generators::default();
    expected.add_generator(&GeneratorCategory::STATUS, Generator::RandomInt(400, 499));
    expect!(generators!{
      "STATUS" => Generator::RandomInt(400, 499)
    }).to(be_equal_to(expected));

    expected = Generators::default();
    expected.add_generator_with_subcategory(&GeneratorCategory::BODY, "$.a.b",
                                            Generator::RandomInt(1, 10));
    expect!(generators!{
      "BODY" => {
        "$.a.b" => Generator::RandomInt(1, 10)
      }
    }).to(be_equal_to(expected));
  }

  #[test]
  fn generator_from_json_test() {
    expect!(Generator::from_map("", &serde_json::Map::new())).to(be_none());
    expect!(Generator::from_map("Invalid", &serde_json::Map::new())).to(be_none());
    expect!(Generator::from_map("uuid", &serde_json::Map::new())).to(be_none());
    expect!(Generator::from_map("Uuid", &serde_json::Map::new())).to(be_some().value(Generator::Uuid));
    expect!(Generator::from_map("RandomBoolean", &serde_json::Map::new())).to(be_some().value(Generator::RandomBoolean));
  }

  #[test]
  fn randomint_generator_from_json_test() {
    expect!(Generator::from_map("RandomInt", &serde_json::Map::new())).to(be_some().value(Generator::RandomInt(0, 10)));
    expect!(Generator::from_map("RandomInt", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomInt(5, 10)));
    expect!(Generator::from_map("RandomInt", &json!({ "max": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomInt(0, 5)));
    expect!(Generator::from_map("RandomInt", &json!({ "min": 5, "max": 6 }).as_object().unwrap())).to(be_some().value(Generator::RandomInt(5, 6)));
    expect!(Generator::from_map("RandomInt", &json!({ "min": 0, "max": 1234567890 }).as_object().unwrap())).to(be_some().value(Generator::RandomInt(0, 1234567890)));
  }

  #[test]
  fn random_decimal_generator_from_json_test() {
    expect!(Generator::from_map("RandomDecimal", &serde_json::Map::new())).to(be_some().value(Generator::RandomDecimal(10)));
    expect!(Generator::from_map("RandomDecimal", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomDecimal(10)));
    expect!(Generator::from_map("RandomDecimal", &json!({ "digits": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomDecimal(5)));
  }

  #[test]
  fn random_hexadecimal_generator_from_json_test() {
    expect!(Generator::from_map("RandomHexadecimal", &serde_json::Map::new())).to(be_some().value(Generator::RandomHexadecimal(10)));
    expect!(Generator::from_map("RandomHexadecimal", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomHexadecimal(10)));
    expect!(Generator::from_map("RandomHexadecimal", &json!({ "digits": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomHexadecimal(5)));
  }

  #[test]
  fn random_string_generator_from_json_test() {
    expect!(Generator::from_map("RandomString", &serde_json::Map::new())).to(be_some().value(Generator::RandomString(10)));
    expect!(Generator::from_map("RandomString", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomString(10)));
    expect!(Generator::from_map("RandomString", &json!({ "size": 5 }).as_object().unwrap())).to(be_some().value(Generator::RandomString(5)));
  }

  #[test]
  fn regex_generator_from_json_test() {
    expect!(Generator::from_map("Regex", &serde_json::Map::new())).to(be_none());
    expect!(Generator::from_map("Regex", &json!({ "min": 5 }).as_object().unwrap())).to(be_none());
    expect!(Generator::from_map("Regex", &json!({ "regex": "\\d+" }).as_object().unwrap())).to(be_some().value(Generator::Regex("\\d+".to_string())));
    expect!(Generator::from_map("Regex", &json!({ "regex": 5 }).as_object().unwrap())).to(be_some().value(Generator::Regex("5".to_string())));
  }

  #[test]
  fn date_generator_from_json_test() {
    expect!(Generator::from_map("Date", &serde_json::Map::new())).to(be_some().value(Generator::Date(None)));
    expect!(Generator::from_map("Date", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::Date(None)));
    expect!(Generator::from_map("Date", &json!({ "format": "yyyy-MM-dd" }).as_object().unwrap())).to(be_some().value(Generator::Date(Some("yyyy-MM-dd".to_string()))));
    expect!(Generator::from_map("Date", &json!({ "format": 5 }).as_object().unwrap())).to(be_some().value(Generator::Date(Some("5".to_string()))));
  }

  #[test]
  fn time_generator_from_json_test() {
    expect!(Generator::from_map("Time", &serde_json::Map::new())).to(be_some().value(Generator::Time(None)));
    expect!(Generator::from_map("Time", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::Time(None)));
    expect!(Generator::from_map("Time", &json!({ "format": "yyyy-MM-dd" }).as_object().unwrap())).to(be_some().value(Generator::Time(Some("yyyy-MM-dd".to_string()))));
    expect!(Generator::from_map("Time", &json!({ "format": 5 }).as_object().unwrap())).to(be_some().value(Generator::Time(Some("5".to_string()))));
  }

  #[test]
  fn datetime_generator_from_json_test() {
    expect!(Generator::from_map("DateTime", &serde_json::Map::new())).to(be_some().value(Generator::DateTime(None)));
    expect!(Generator::from_map("DateTime", &json!({ "min": 5 }).as_object().unwrap())).to(be_some().value(Generator::DateTime(None)));
    expect!(Generator::from_map("DateTime", &json!({ "format": "yyyy-MM-dd" }).as_object().unwrap())).to(be_some().value(Generator::DateTime(Some("yyyy-MM-dd".to_string()))));
    expect!(Generator::from_map("DateTime", &json!({ "format": 5 }).as_object().unwrap())).to(be_some().value(Generator::DateTime(Some("5".to_string()))));
  }

  #[test]
  fn provider_state_generator_from_json_test() {
    expect!(Generator::from_map("ProviderState", &serde_json::Map::new())).to(be_none());
    expect!(Generator::from_map("ProviderState", &json!({ "expression": "5" }).as_object().unwrap())).to(
      be_some().value(Generator::ProviderStateGenerator("5".into(), None)));
    expect!(Generator::from_map("ProviderState", &json!({ "expression": "5", "dataType": "INTEGER" }).as_object().unwrap())).to(
      be_some().value(Generator::ProviderStateGenerator("5".into(), Some(DataType::INTEGER))));
  }

  #[test]
  fn generator_to_json_test() {
    expect!(Generator::RandomInt(5, 15).to_json().unwrap()).to(be_equal_to(json!({
      "type": "RandomInt",
      "min": 5,
      "max": 15
    })));
    expect!(Generator::Uuid.to_json().unwrap()).to(be_equal_to(json!({
      "type": "Uuid"
    })));
    expect!(Generator::RandomDecimal(5).to_json().unwrap()).to(be_equal_to(json!({
      "type": "RandomDecimal",
      "digits": 5
    })));
    expect!(Generator::RandomHexadecimal(5).to_json().unwrap()).to(be_equal_to(json!({
      "type": "RandomHexadecimal",
      "digits": 5
    })));
    expect!(Generator::RandomString(5).to_json().unwrap()).to(be_equal_to(json!({
      "type": "RandomString",
      "size": 5
    })));
    expect!(Generator::Regex("\\d+".into()).to_json().unwrap()).to(be_equal_to(json!({
      "type": "Regex",
      "regex": "\\d+"
    })));
    expect!(Generator::RandomBoolean.to_json().unwrap()).to(be_equal_to(json!({
      "type": "RandomBoolean"
    })));

    expect!(Generator::Date(Some("yyyyMMdd".into())).to_json().unwrap()).to(be_equal_to(json!({
      "type": "Date",
      "format": "yyyyMMdd"
    })));
    expect!(Generator::Date(None).to_json().unwrap()).to(be_equal_to(json!({
      "type": "Date"
    })));
    expect!(Generator::Time(Some("yyyyMMdd".into())).to_json().unwrap()).to(be_equal_to(json!({
      "type": "Time",
      "format": "yyyyMMdd"
    })));
    expect!(Generator::Time(None).to_json().unwrap()).to(be_equal_to(json!({
      "type": "Time"
    })));
    expect!(Generator::DateTime(Some("yyyyMMdd".into())).to_json().unwrap()).to(be_equal_to(json!({
      "type": "DateTime",
      "format": "yyyyMMdd"
    })));
    expect!(Generator::DateTime(None).to_json().unwrap()).to(be_equal_to(json!({
      "type": "DateTime"
    })));
    expect!(Generator::ProviderStateGenerator("$a".into(), Some(DataType::INTEGER)).to_json().unwrap()).to(be_equal_to(json!({
      "type": "ProviderState",
      "expression": "$a",
      "dataType": "INTEGER"
    })));
    expect!(Generator::ProviderStateGenerator("$a".into(), None).to_json().unwrap()).to(be_equal_to(json!({
      "type": "ProviderState",
      "expression": "$a"
    })));
    expect!(Generator::MockServerURL("http://localhost:1234/path".into(), "(.*)/path".into()).to_json().unwrap()).to(be_equal_to(json!({
      "type": "MockServerURL",
      "example": "http://localhost:1234/path",
      "regex": "(.*)/path"
    })));
  }

  #[test]
  fn generators_to_json_test() {
    let mut generators = Generators::default();
    generators.add_generator(&GeneratorCategory::STATUS, RandomInt(200, 299));
    generators.add_generator(&GeneratorCategory::PATH, Regex("\\d+".into()));
    generators.add_generator(&GeneratorCategory::METHOD, RandomInt(200, 299));
    generators.add_generator_with_subcategory(&GeneratorCategory::BODY, "$.1", RandomDecimal(4));
    generators.add_generator_with_subcategory(&GeneratorCategory::BODY, "$.2", RandomDecimal(4));
    generators.add_generator_with_subcategory(&GeneratorCategory::HEADER, "A", RandomDecimal(4));
    generators.add_generator_with_subcategory(&GeneratorCategory::HEADER, "B", RandomDecimal(4));
    generators.add_generator_with_subcategory(&GeneratorCategory::QUERY, "a", RandomDecimal(4));
    generators.add_generator_with_subcategory(&GeneratorCategory::QUERY, "b", RandomDecimal(4));
    let json = generators.to_json();
    expect(json).to(be_equal_to(json!({
      "body": {
        "$.1": {"digits": 4, "type": "RandomDecimal"},
        "$.2": {"digits": 4, "type": "RandomDecimal"}
      },
      "header": {
        "A": {"digits": 4, "type": "RandomDecimal"},
        "B": {"digits": 4, "type": "RandomDecimal"}
      },
      "method": {"max": 299, "min": 200, "type": "RandomInt"},
      "path": {"regex": "\\d+", "type": "Regex"},
      "query": {
        "a": {"digits": 4, "type": "RandomDecimal"},
        "b": {"digits": 4, "type": "RandomDecimal"}
      },
      "status": {"max": 299, "min": 200, "type": "RandomInt"}
    })));
  }

  #[test]
  fn generate_decimal_test() {
    assert_that!(generate_decimal(4), matches_regex(r"^\d{1,3}\.\d{1,3}$"));
    assert_that!(generate_hexadecimal(4), matches_regex(r"^[0-9A-F]{4}$"));
  }

  #[test]
  fn generate_int_with_max_int_test() {
    assert_that!(Generator::RandomInt(0, i32::max_value()).generate_value(&0,
      &hashmap!{}).unwrap().to_string(), matches_regex(r"^\d+$"));
  }

  #[test]
  fn provider_state_generator_test() {
    expect!(Generator::ProviderStateGenerator("${a}".into(), Some(DataType::INTEGER)).generate_value(&0,
      &hashmap!{ "a".into() => json!(1234) })).to(be_ok().value(1234));
  }

  #[test]
  fn date_generator_test() {
    let generated = Generator::Date(None).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^\d{4}-\d{2}-\d{2}$"));

    let generated2 = Generator::Date(Some("yyyy-MM-ddZ".into())).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated2.unwrap(), matches_regex(r"^\d{4}-\d{2}-\d{2}[-+]\d{4}$"));
  }

  #[test]
  fn time_generator_test() {
    let generated = Generator::Time(None).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^\d{2}:\d{2}:\d{2}$"));

    let generated2 = Generator::Time(Some("HH:mm:ssZ".into())).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated2.unwrap(), matches_regex(r"^\d{2}:\d{2}:\d{2}[-+]\d+$"));
  }

  #[test]
  fn datetime_generator_test() {
    let generated = Generator::DateTime(None).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}[-+]\d+$"));

    let generated2 = Generator::DateTime(Some("yyyy-MM-dd HH:mm:ssZ".into())).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated2.unwrap(), matches_regex(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}[-+]\d+$"));
  }

  #[test]
  fn regex_generator_test() {
    let generated = Generator::Regex(r"\d{4}\w{1,4}".into()).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^\d{4}\w{1,4}$"));

    let generated = Generator::Regex(r"\d{1,2}/\d{1,2}".into()).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^\d{1,2}/\d{1,2}$"));

    let generated = Generator::Regex(r"^\d{1,2}/\d{1,2}$".into()).generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^\d{1,2}/\d{1,2}$"));
  }

  #[test]
  fn uuid_generator_test() {
    let generated = Generator::Uuid.generate_value(&"".to_string(), &hashmap!{});
    assert_that!(generated.unwrap(), matches_regex(r"^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$"));
  }

  #[test]
  fn random_decimal_generator_test() {
    for _ in 1..10 {
      let generated = Generator::RandomDecimal(10).generate_value(&"".to_string(), &hashmap! {}).unwrap();
      expect!(generated.clone().len()).to(be_equal_to(11));
      assert_that!(generated.clone(), matches_regex(r"^\d+\.\d+$"));
      let mut chars = generated.chars();
      let first_char = chars.next().unwrap();
      let second_char = chars.next().unwrap();
      println!("{}: '{}' != '0' || ('{}' == '0' && '{}' == '.')", generated, first_char, first_char, second_char);
      expect!(first_char != '0' || (first_char == '0' && second_char == '.')).to(be_true());
    }
  }

  #[test]
  fn handle_edge_case_when_digits_is_1() {
    let generated = Generator::RandomDecimal(1).generate_value(&"".to_string(), &hashmap! {}).unwrap();
    assert_that!(generated, matches_regex(r"^\d$"));
  }

  #[test]
  fn handle_edge_case_when_digits_is_2() {
    let generated = Generator::RandomDecimal(2).generate_value(&"".to_string(), &hashmap! {}).unwrap();
    assert_that!(generated, matches_regex(r"^\d\.\d$"));
  }

  #[test]
  fn mock_server_url_generator_test() {
    let generator = Generator::MockServerURL("http://localhost:1234/path".into(), ".*(/path)$".into());
    let generated = generator.generate_value(&"".to_string(), &hashmap!{
        "mockServer" => json!({
          "url": "http://192.168.2.1:2345/p",
          "port": 2345
        })
      });
    expect!(generated.unwrap()).to(be_equal_to("http://192.168.2.1:2345/p/path"));
    let generated = generator.generate_value(&"".to_string(), &hashmap!{});
    expect!(generated).to(be_err());
  }
}