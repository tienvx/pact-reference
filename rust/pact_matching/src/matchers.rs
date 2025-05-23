//! Matching rule implementations

use std::str::from_utf8;

use anyhow::anyhow;
use bytes::Bytes;
#[cfg(feature = "plugins")] use lazy_static::lazy_static;
#[cfg(feature = "plugins")] use maplit::hashmap;
use onig::Regex;
use pact_models::HttpStatus;
use pact_models::matchingrules::{MatchingRule, RuleList, RuleLogic};
use pact_models::path_exp::DocPath;
#[cfg(feature = "datetime")] use pact_models::time_utils::validate_datetime;
#[cfg(feature = "plugins")]  use pact_plugin_driver::catalogue_manager::{
  CatalogueEntry,
  CatalogueEntryProviderType,
  CatalogueEntryType,
  register_core_entries
};
use semver::Version;
use tracing::{debug, instrument, trace};

use crate::binary_utils::match_content_type;
use crate::{MatchingContext, CommonMismatch};

#[cfg(feature = "plugins")]
lazy_static! {
  /// Content matcher/generator entries to add to the plugin catalogue
  static ref CONTENT_MATCHER_CATALOGUE_ENTRIES: Vec<CatalogueEntry> = {
    let mut entries = vec![];
    entries.push(CatalogueEntry {
      entry_type: CatalogueEntryType::CONTENT_MATCHER,
      provider_type: CatalogueEntryProviderType::CORE,
      plugin: None,
      key: "xml".to_string(),
      values: hashmap!{
        "content-types".to_string() => "application/.*xml,text/xml".to_string()
      }
    });
    entries.push(CatalogueEntry {
      entry_type: CatalogueEntryType::CONTENT_MATCHER,
      provider_type: CatalogueEntryProviderType::CORE,
      plugin: None,
      key: "json".to_string(),
      values: hashmap!{
        "content-types".to_string() => "application/.*json,application/json-rpc,application/jsonrequest".to_string()
      }
    });
    entries.push(CatalogueEntry {
      entry_type: CatalogueEntryType::CONTENT_MATCHER,
      provider_type: CatalogueEntryProviderType::CORE,
      plugin: None,
      key: "text".to_string(),
      values: hashmap!{
        "content-types".to_string() => "text/plain".to_string()
      }
    });
    entries.push(CatalogueEntry {
      entry_type: CatalogueEntryType::CONTENT_MATCHER,
      provider_type: CatalogueEntryProviderType::CORE,
      plugin: None,
      key: "multipart-form-data".to_string(),
      values: hashmap!{
        "content-types".to_string() => "multipart/form-data,multipart/mixed".to_string()
      }
    });
    // TODO:
    // entries.push(CatalogueEntry {
    //   entry_type: CatalogueEntryType::CONTENT_MATCHER,
    //   provider_type: CatalogueEntryProviderType::CORE,
    //   plugin: None,
    //   key: "form-urlencoded".to_string(),
    //   values: hashmap!{
    //     "content-types".to_string() => "application/x-www-form-urlencoded".to_string()
    //   }
    // });
    entries.push(CatalogueEntry {
      entry_type: CatalogueEntryType::CONTENT_GENERATOR,
      provider_type: CatalogueEntryProviderType::CORE,
      plugin: None,
      key: "json".to_string(),
      values: hashmap!{
        "content-types".to_string() => "application/.*json,application/json-rpc,application/jsonrequest".to_string()
      }
    });
    entries.push(CatalogueEntry {
      entry_type: CatalogueEntryType::CONTENT_GENERATOR,
      provider_type: CatalogueEntryProviderType::CORE,
      plugin: None,
      key: "binary".to_string(),
      values: hashmap!{
        "content-types".to_string() => "application/octet-stream".to_string()
      }
    });
    entries
  };

  static ref MATCHER_CATALOGUE_ENTRIES: Vec<CatalogueEntry> = {
    let mut entries = vec![];
    for matcher in ["v2-regex", "v2-type", "v3-number-type", "v3-integer-type", "v3-decimal-type",
      "v3-date", "v3-time", "v3-datetime", "v2-min-type", "v2-max-type", "v2-minmax-type",
      "v3-includes", "v3-null", "v4-equals-ignore-order", "v4-min-equals-ignore-order",
      "v4-max-equals-ignore-order", "v4-minmax-equals-ignore-order", "v3-content-type",
      "v4-array-contains", "v1-equality", "v4-not-empty", "v4-semver"] {
      entries.push(CatalogueEntry {
        entry_type: CatalogueEntryType::MATCHER,
        provider_type: CatalogueEntryProviderType::CORE,
        plugin: None,
        key: matcher.to_string(),
        values: hashmap!{}
      });
    }
    entries
  };
}

/// Sets up all the core catalogue entries for matchers and generators
pub fn configure_core_catalogue() {
  #[cfg(feature = "plugins")] register_core_entries(CONTENT_MATCHER_CATALOGUE_ENTRIES.as_ref());
  #[cfg(feature = "plugins")] register_core_entries(MATCHER_CATALOGUE_ENTRIES.as_ref());
}

/// Trait for matching rule implementation
pub trait Matches<A: Clone> {
  /// If the actual value matches self given the matching rule
  #[deprecated(since = "0.9.2", note="Use matches_with instead")]
  fn matches(&self, actual: &A, matcher: &MatchingRule) -> anyhow::Result<()> {
    self.matches_with(actual.clone(), matcher, false)
  }

  /// If the actual value matches self given the matching rule
  fn matches_with(&self, actual: A, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()>;
}

impl Matches<String> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: String, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.as_str().matches_with(actual.as_str(), matcher, cascaded)
  }
}

impl Matches<&String> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: &String, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.as_str().matches_with(actual.as_str(), matcher, cascaded)
  }
}

impl Matches<&String> for &String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: &String, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.as_str().matches_with(actual.as_str(), matcher, cascaded)
  }
}

impl Matches<&str> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: &str, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.as_str().matches_with(actual, matcher, cascaded)
  }
}

impl Matches<&str> for &str {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: &str, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    let result = match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(actual) {
              Ok(())
            } else {
              Err(anyhow!("Expected '{}' to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Equality | MatchingRule::Values => {
        if self == &actual {
          Ok(())
        } else {
          Err(anyhow!("Expected '{}' to be equal to '{}'", actual, self))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) => Ok(()),
      MatchingRule::Include(substr) => {
        if actual.contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected '{}' to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Decimal => {
        match actual.parse::<f64>() {
          Ok(_) => Ok(()),
          Err(_) => Err(anyhow!("Expected '{}' to match a number", actual))
        }
      },
      MatchingRule::Integer => {
        match actual.parse::<u64>() {
          Ok(_) => Ok(()),
          Err(_) => Err(anyhow!("Expected '{}' to match an integer number", actual))
        }
      },
      #[allow(unused_variables)]
      MatchingRule::Date(s) => {
        #[cfg(feature = "datetime")]
        {
          let format = if s.is_empty() {
            "yyyy-MM-dd"
          } else {
            s.as_str()
          };
          match validate_datetime(&actual.to_string(), format) {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!("Expected '{}' to match a date pattern of '{}'", actual, format))
          }
        }
        #[cfg(not(feature = "datetime"))]
        {
          Err(anyhow!("Date matchers require the datetime feature to be enabled"))
        }
      },
      #[allow(unused_variables)]
      MatchingRule::Time(s) => {
        #[cfg(feature = "datetime")]
        {
          let format = if s.is_empty() {
            "HH:mm:ss"
          } else {
            s.as_str()
          };
          match validate_datetime(&actual.to_string(), format) {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!("Expected '{}' to match a time pattern of '{}'", actual, format))
          }
        }
        #[cfg(not(feature = "datetime"))]
        {
          Err(anyhow!("Time matchers require the datetime feature to be enabled"))
        }
      },
      #[allow(unused_variables)]
      MatchingRule::Timestamp(s) => {
        #[cfg(feature = "datetime")]
        {
          let format = if s.is_empty() {
            "yyyy-MM-dd'T'HH:mm:ssXXX"
          } else {
            s.as_str()
          };
          match validate_datetime(&actual.to_string(), format) {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!("Expected '{}' to match a timestamp pattern of '{}'", actual, format))
          }
        }
        #[cfg(not(feature = "datetime"))]
        {
          Err(anyhow!("DateTime matchers require the datetime feature to be enabled"))
        }
      },
      MatchingRule::Boolean => {
        if actual == "true" || actual == "false" {
          Ok(())
        } else {
          Err(anyhow!("Expected '{}' (String) to match a boolean", actual))
        }
      }
      MatchingRule::StatusCode(status) => {
        match actual.parse::<u16>() {
          Ok(status_code) => match_status_code(status_code, status),
          Err(err) => Err(anyhow!("Unable to match '{}' using {:?} - {}", self, matcher, err))
        }
      }
      MatchingRule::NotEmpty => {
        if actual.is_empty() {
          Err(anyhow!("Expected '' (String) to not be empty"))
        } else {
          Ok(())
        }
      }
      MatchingRule::Semver => {
        match Version::parse(actual) {
          Ok(_) => Ok(()),
          Err(err) => Err(anyhow!("'{}' is not a valid semantic version - {}", actual, err))
        }
      }
      MatchingRule::ContentType(content_type) => match_content_type(actual.as_bytes(), content_type),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match '{}' using {:?}", self, matcher))
      } else {
        Ok(())
      }
    };
    debug!(cascaded, ?matcher, "String -> String: comparing '{}' to '{}' ==> {}", self, actual, result.is_ok());
    result
  }
}

impl Matches<u64> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.as_str().matches_with(actual, matcher, cascaded)
  }
}

impl Matches<u64> for &str {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("String -> u64: comparing '{}' to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) =>
        Err(anyhow!("Expected '{}' (String) to be the same type as {} (Number)", self, actual)),
      MatchingRule::Equality => Err(anyhow!("Expected {} (Number) to be equal to '{}' (String)", actual, self)),
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Integer => Ok(()),
      MatchingRule::Decimal => Err(anyhow!("Expected {} to match a decimal number", actual)),
      MatchingRule::StatusCode(status) => match_status_code(actual as u16, status),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("String: Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<u64> for u64 {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("u64 -> u64: comparing {} to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) => Ok(()),
      MatchingRule::Equality => {
        if *self == actual {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to be equal to {}", actual, self))
        }
      },
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Integer => Ok(()),
      MatchingRule::Decimal => Err(anyhow!("Expected {} to match a decimal number", actual)),
      MatchingRule::StatusCode(status) => match_status_code(actual as u16, status),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<f64> for u64 {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: f64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("u64 -> f64: comparing {} to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) =>
        Err(anyhow!("Expected {} (Integer) to be the same type as {} (Decimal)", self, actual)),
      MatchingRule::Equality => Err(anyhow!("Expected {} (Decimal) to be equal to {} (Integer)", actual, self)),
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Decimal => Ok(()),
      MatchingRule::Integer => Err(anyhow!("Expected {} to match an integer number", actual)),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<f64> for f64 {
  #[allow(clippy::float_cmp)]
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: f64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    let result = match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) => Ok(()),
      MatchingRule::Equality => {
        if *self == actual {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to be equal to {}", actual, self))
        }
      },
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Decimal => Ok(()),
      MatchingRule::Integer => Err(anyhow!("Expected {} to match an integer number", actual)),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    };
    debug!("f64 -> f64: comparing {} to {} using {:?} == {:?}", self, actual, matcher, result);
    result
  }
}

impl Matches<u64> for f64 {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("f64 -> u64: comparing {} to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected '{}' to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) =>
        Err(anyhow!("Expected {} (Decimal) to be the same type as {} (Integer)", self, actual)),
      MatchingRule::Equality => Err(anyhow!("Expected {} (Integer) to be equal to {} (Decimal)", actual, self)),
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Integer => Ok(()),
      MatchingRule::Decimal => Err(anyhow!("Expected {} to match a decimal number", actual)),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match '{}' using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<u16> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u16, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("String -> u16: comparing '{}' to {} using {:?}", self, actual, matcher);
    self.matches_with(actual as u64, matcher, cascaded)
  }
}

impl Matches<u16> for &str {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u16, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("String -> u16: comparing '{}' to {} using {:?}", self, actual, matcher);
    self.matches_with(actual as u64, matcher, cascaded)
  }
}

impl Matches<u16> for u16 {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: u16, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("u16 -> u16: comparing {} to {} using {:?}", self, actual, matcher);
    (*self as u64).matches_with(actual as u64, matcher, cascaded)
  }
}

impl Matches<i64> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: i64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("String -> i64: comparing {} to {} using {:?}", self, actual, matcher);
    self.as_str().matches_with(actual, matcher, cascaded)
  }
}

impl Matches<i64> for &str {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: i64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("String -> i64: comparing '{}' to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) =>
        Err(anyhow!("Expected '{}' (String) to be the same type as {} (Number)", self, actual)),
      MatchingRule::Equality => Err(anyhow!("Expected {} (Number) to be equal to '{}' (String)", actual, self)),
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Integer => Ok(()),
      MatchingRule::Decimal => Err(anyhow!("Expected {} to match a decimal number", actual)),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<i64> for i64 {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: i64, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("i64 -> i64: comparing {} to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) => Ok(()),
      MatchingRule::Equality => {
        if *self == actual {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to be equal to {}", actual, self))
        }
      },
      MatchingRule::Include(substr) => {
        if actual.to_string().contains(substr) {
          Ok(())
        } else {
          Err(anyhow!("Expected {} to include '{}'", actual, substr))
        }
      },
      MatchingRule::Number | MatchingRule::Integer => Ok(()),
      MatchingRule::Decimal => Err(anyhow!("Expected {} to match a decimal number", actual)),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<i32> for String {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: i32, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.matches_with(actual as i64, matcher, cascaded)
  }
}

impl Matches<i32> for &str {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: i32, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.matches_with(actual as i64, matcher, cascaded)
  }
}

impl Matches<i32> for i32 {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: i32, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    (*self as i64).matches_with(actual as i64, matcher, cascaded)
  }
}

impl Matches<bool> for bool {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: bool, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("bool -> bool: comparing '{}' to {} using {:?}", self, actual, matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            if re.is_match(&actual.to_string()) {
              Ok(())
            } else {
              Err(anyhow!("Expected {} to match '{}'", actual, regex))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) => Ok(()),
      MatchingRule::Equality => if actual == *self {
        Ok(())
      } else {
        Err(anyhow!("Expected {} (Boolean) to be equal to {} (Boolean)", actual, self))
      },
      MatchingRule::Boolean => Ok(()),
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Boolean: Unable to match {} using {:?}", self, matcher))
      } else {
        Ok(())
      }
    }
  }
}

impl Matches<Bytes> for Bytes {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: Bytes, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    self.matches_with(&actual, matcher, cascaded)
  }
}

impl Matches<&Bytes> for Bytes {
  #[instrument(level = "trace")]
  fn matches_with(&self, actual: &Bytes, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    debug!("Bytes -> Bytes: comparing {} bytes to {} bytes using {:?}", self.len(), actual.len(), matcher);
    match matcher {
      MatchingRule::Regex(regex) => {
        match Regex::new(regex) {
          Ok(re) => {
            match from_utf8(actual) {
              Ok(s) => if re.is_match(s) {
                Ok(())
              } else {
                Err(anyhow!("Expected '{}' to match '{}'", s, regex))
              }
              Err(err) => Err(anyhow!("Could not convert actual bytes into a UTF-8 string - {}", err))
            }
          },
          Err(err) => Err(anyhow!("'{}' is not a valid regular expression - {}", regex, err))
        }
      },
      MatchingRule::Equality => {
        if self == actual {
          Ok(())
        } else {
          Err(anyhow!("Expected '{:?}...' ({} bytes) to be equal to '{:?}...' ({} bytes)",
                      self.split_at(10).0, self.len(), actual.split_at(10).0, actual.len()))
        }
      },
      MatchingRule::Type |
      MatchingRule::MinType(_) |
      MatchingRule::MaxType(_) |
      MatchingRule::MinMaxType(_, _) => Ok(()),
      MatchingRule::Include(substr) => {
        match from_utf8(actual) {
          Ok(s) => if s.contains(substr) {
            Ok(())
          } else {
            Err(anyhow!("Expected '{}' to include '{}'", s, substr))
          }
          Err(err) => Err(anyhow!("Could not convert actual bytes into a UTF-8 string - {}", err))
        }
      },
      MatchingRule::ContentType(content_type) => match_content_type(actual, content_type),
      MatchingRule::NotEmpty => {
        if actual.is_empty() {
          Err(anyhow!("Expected [] (0 bytes) to not be empty"))
        } else {
          Ok(())
        }
      }
      _ => if !cascaded || matcher.can_cascade() {
        Err(anyhow!("Unable to match '{:?}...' ({} bytes) using {:?}", actual.split_at(10).0, actual.len(), matcher))
      } else {
        Ok(())
      }
    }
  }
}

/// Match the provided values using the path and matching rules
pub fn match_values<E, A>(path: &DocPath, matching_rules: &RuleList, expected: E, actual: A) -> Result<(), Vec<String>>
  where E: Matches<A>, A: Clone {
  trace!("match_values: {} -> {}", std::any::type_name::<E>(), std::any::type_name::<A>());
  if matching_rules.is_empty() {
    Err(vec![format!("No matcher found for path '{}'", path)])
  } else {
    let results = matching_rules.rules.iter().map(|rule| {
      expected.matches_with(actual.clone(), rule, matching_rules.cascaded)
    }).collect::<Vec<anyhow::Result<()>>>();
    let result = match matching_rules.rule_logic {
      RuleLogic::And => {
        if results.iter().all(|result| result.is_ok()) {
          Ok(())
        } else {
          Err(results.iter().filter(|result| result.is_err())
            .map(|result| result.as_ref().unwrap_err().to_string()).collect())
        }
      },
      RuleLogic::Or => {
        if results.iter().any(|result| result.is_ok()) {
          Ok(())
        } else {
          Err(results.iter().filter(|result| result.is_err())
            .map(|result| result.as_ref().unwrap_err().to_string()).collect())
        }
      }
    };
    trace!(?result, "match_values: {} -> {}", std::any::type_name::<E>(), std::any::type_name::<A>());
    result
  }
}

#[instrument(level = "trace")]
fn match_status_code(status_code: u16, status: &HttpStatus) -> anyhow::Result<()> {
  let matches = match status {
    HttpStatus::Information => (100..=199).contains(&status_code),
    HttpStatus::Success => (200..=299).contains(&status_code),
    HttpStatus::Redirect => (300..=399).contains(&status_code),
    HttpStatus::ClientError => (400..=499).contains(&status_code),
    HttpStatus::ServerError => (500..=599).contains(&status_code),
    HttpStatus::StatusCodes(status_codes) => status_codes.contains(&status_code),
    HttpStatus::NonError => status_code < 400,
    HttpStatus::Error => status_code >= 400
  };
  let result = if matches {
    Ok(())
  } else {
    Err(anyhow!("Expected status code {} to be a {}", status_code, status))
  };
  trace!(status_code, ?status, matches, ?result, "matching status code");
  result
}

/// Basic matching implementation for string slices
pub fn match_strings(
  path: &DocPath,
  expected: &str,
  actual: &str,
  context: &dyn MatchingContext
) -> Result<(), Vec<CommonMismatch>> {
  let matcher_result = if context.matcher_is_defined(&path) {
    debug!("Calling match_values for path {}", path);
    match_values(&path, &context.select_best_matcher(&path), expected, actual)
  } else {
    expected.matches_with(actual, &MatchingRule::Equality, false).map_err(|err|
      vec![format!("String '{}': {}", path, err)]
    )
  };
  debug!("Comparing '{:?}' to '{:?}' at path '{}' -> {:?}", expected, actual, path, matcher_result);
  matcher_result.map_err(|messages| {
    messages.iter().map(|message| {
      CommonMismatch {
        path: path.to_string(),
        expected: expected.to_string(),
        actual: actual.to_string(),
        description: message.clone()
      }
    }).collect()
  })
}

#[cfg(test)]
mod tests {
  use expectest::expect;
  use expectest::prelude::*;
  use pact_models::{matchingrules, matchingrules::RuleList, matchingrules_list};
  use serde_json::json;

  use super::*;

  #[test]
  fn select_best_matcher_selects_most_appropriate_by_weight() {
    let matchers = matchingrules! {
      "body" => {
        "$" => [ MatchingRule::Regex("1".to_string()) ],
        "$.item1" => [ MatchingRule::Regex("3".to_string()) ],
        "$.item2" => [ MatchingRule::Regex("4".to_string()) ],
        "$.item1.level" => [ MatchingRule::Regex("6".to_string()) ],
        "$.item1.level[1]" => [ MatchingRule::Regex("7".to_string()) ],
        "$.item1.level[1].id" => [ MatchingRule::Regex("8".to_string()) ],
        "$.item1.level[1].name" => [ MatchingRule::Regex("9".to_string()) ],
        "$.item1.level[2]" => [ MatchingRule::Regex("10".to_string()) ],
        "$.item1.level[2].id" => [ MatchingRule::Regex("11".to_string()) ],
        "$.item1.level[*].id" => [ MatchingRule::Regex("12".to_string()) ],
        "$.*.level[*].id" => [ MatchingRule::Regex("13".to_string()) ]
      },
      "header" => {
        "item1" => [ MatchingRule::Regex("5".to_string()) ]
      }
    };
    let body_matchers = matchers.rules_for_category("body").unwrap();
    let header_matchers = matchers.rules_for_category("header").unwrap();

    expect!(body_matchers.select_best_matcher(&vec!["$"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("1".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "a"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("1".to_string()))));

    expect!(body_matchers.select_best_matcher(&vec!["$", "item1"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("3".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item2"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("4".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item3"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("1".to_string()))));

    expect!(header_matchers.select_best_matcher(&vec!["$", "item1"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("5".to_string()))));

    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("6".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "1"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("7".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "2"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("10".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "1", "id"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("8".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "1", "name"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("9".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "1", "other"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("7".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "2", "id"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("11".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item1", "level", "3", "id"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("12".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item2", "level", "1", "id"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("13".to_string()))));
    expect!(body_matchers.select_best_matcher(&vec!["$", "item2", "level", "3", "id"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("13".to_string()))));
  }

  #[test]
  fn select_best_matcher_selects_most_appropriate_when_weight_is_equal() {
    let matchers = matchingrules!{
      "body" => {
          "$.animals" => [ MatchingRule::Regex("1".to_string()) ],
          "$.animals.*" => [ MatchingRule::Regex("2".to_string()) ],
          "$.animals.*.alligator['@phoneNumber']" => [ MatchingRule::Regex("3".to_string()) ]
      },
      "header" => {
          "item1" => [ MatchingRule::Regex("5".to_string()) ]
      }
    };
    let body_matchers = matchers.rules_for_category("body").unwrap();

    expect!(body_matchers.select_best_matcher(&vec!["$", "animals", "0"])).to(
      be_equal_to(RuleList::new(MatchingRule::Regex("2".to_string()))));
  }

    #[test]
    fn select_best_matcher_selects_handles_missing_type_attribute() {
      let matchers = matchingrules_list! {
        "body";
        "$.item1" => [ MatchingRule::Regex("3".to_string()) ],
        "$.item2" => [ MatchingRule::MinType(4) ],
        "$.item3" => [ MatchingRule::MaxType(4) ],
        "$.item4" => [ ]
      };

      expect!(matchers.select_best_matcher(&vec!["$", "item1"])).to(
        be_equal_to(RuleList::new(MatchingRule::Regex("3".to_string()))));
      expect!(matchers.select_best_matcher(&vec!["$", "item2"])).to(
        be_equal_to(RuleList::new(MatchingRule::MinType(4))));
      expect!(matchers.select_best_matcher(&vec!["$", "item3"])).to(
        be_equal_to(RuleList::new(MatchingRule::MaxType(4))));
      expect!(matchers.select_best_matcher(&vec!["$", "item4"]).is_empty()).to(be_true());
    }

    #[test]
    fn equality_matcher_test() {
        let matcher = MatchingRule::Equality;
        expect!("100".matches_with("100", &matcher, false)).to(be_ok());
        expect!("100".matches_with("101", &matcher, false)).to(be_err());
        expect!("100".matches_with(100, &matcher, false)).to(be_err());
        expect!(100.matches_with(100, &matcher, false)).to(be_ok());
        expect!(100.matches_with(100.0, &matcher, false)).to(be_err());
        expect!(100.1f64.matches_with(100.0, &matcher, false)).to(be_err());
    }

    #[test]
    fn regex_matcher_test() {
      let matcher = MatchingRule::Regex("^\\d+$".to_string());
      expect!("100".matches_with("100", &matcher, false)).to(be_ok());
      expect!("100".matches_with("10a", &matcher, false)).to(be_err());
      expect!("100".matches_with(100, &matcher, false)).to(be_ok());
      expect!(100.matches_with(100, &matcher, false)).to(be_ok());
      expect!(100.matches_with(100.01f64, &matcher, false)).to(be_err());
      expect!(100.1f64.matches_with(100.02f64, &matcher, false)).to(be_err());

      // Test for Issue #214
      let matcher = MatchingRule::Regex("^Greater|GreaterOrEqual$".to_string());
      expect!("Greater".matches_with("Greater", &matcher, false)).to(be_ok());
      expect!("Greater".matches_with("GreaterOrEqual", &matcher, false)).to(be_ok());
    }

    #[test]
    fn type_matcher_test() {
        let matcher = MatchingRule::Type;
        expect!("100".matches_with("100", &matcher, false)).to(be_ok());
        expect!("100".matches_with("10a", &matcher, false)).to(be_ok());
        expect!("100".matches_with(100, &matcher, false)).to(be_err());
        expect!(100.matches_with(200, &matcher, false)).to(be_ok());
        expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
        expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
    }

    #[test]
    fn min_type_matcher_test() {
        let matcher = MatchingRule::MinType(3);
        expect!("100".matches_with("100", &matcher, false)).to(be_ok());
        expect!("100".matches_with("10a", &matcher, false)).to(be_ok());
        expect!("100".matches_with("10", &matcher, false)).to(be_ok());
        expect!("100".matches_with(100, &matcher, false)).to(be_err());
        expect!(100.matches_with(200, &matcher, false)).to(be_ok());
        expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
        expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
    }

    #[test]
    fn max_type_matcher_test() {
        let matcher = MatchingRule::MaxType(3);
        expect!("100".matches_with("100", &matcher, false)).to(be_ok());
        expect!("100".matches_with("10a", &matcher, false)).to(be_ok());
        expect!("100".matches_with("1000", &matcher, false)).to(be_ok());
        expect!("100".matches_with(100, &matcher, false)).to(be_err());
        expect!(100.matches_with(200, &matcher, false)).to(be_ok());
        expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
        expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
    }

  #[test]
  fn minmax_type_matcher_test() {
    let matcher = MatchingRule::MinMaxType(3, 6);
    expect!("100".matches_with("100", &matcher, false)).to(be_ok());
    expect!("100".matches_with("10a", &matcher, false)).to(be_ok());
    expect!("100".matches_with("1000", &matcher, false)).to(be_ok());
    expect!("100".matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(200, &matcher, false)).to(be_ok());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
  }

  #[test]
  #[cfg(feature = "datetime")]
  fn timestamp_matcher_test() {
    let matcher = MatchingRule::Timestamp("yyyy-MM-dd HH:mm:ssZZZ".into());

    expect!("100".matches_with("2013-12-01 14:00:00+10:00", &matcher, false)).to(be_err());
    expect!("100".matches_with("2013-12-01 14:00:00+1000", &matcher, false)).to(be_ok());
    expect!("100".matches_with("13-12-01 14:00:00+10:00", &matcher, false)).to(be_err());
    expect!("100".matches_with("I\'m a timestamp!", &matcher, false)).to(be_err());
    expect!("100".matches_with("100", &matcher, false)).to(be_err());
    expect!("100".matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".matches_with("1000", &matcher, false)).to(be_err());
    expect!("100".matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(200, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_err());

    let matcher = MatchingRule::Timestamp("yyyy-MM-dd HH:mm:ssXXX".into());
    expect!("2014-01-01 14:00:00+10:00".matches_with("2013-12-01 14:00:00+10:00", &matcher, false)).to(be_ok());

    let matcher = MatchingRule::Timestamp("yyyy#MM#dd#HH#mm#ss".into());
    expect!("2014-01-01 14:00:00+10:00".matches_with("2013#12#01#14#00#00", &matcher, false)).to(be_ok());

    let matcher = MatchingRule::Timestamp("".into());
    expect!("".matches_with("2013-12-01T14:00:00+10:00", &matcher, false)).to(be_ok());
  }

  #[test]
  #[cfg(feature = "datetime")]
  fn time_matcher_test() {
    let matcher = MatchingRule::Time("HH:mm:ss".into());

    expect!("00:00:00".matches_with("14:00:00", &matcher, false)).to(be_ok());
    expect!("00:00:00".matches_with("33:00:00", &matcher, false)).to(be_err());
    expect!("00:00:00".matches_with("100", &matcher, false)).to(be_err());
    expect!("00:00:00".matches_with("10a", &matcher, false)).to(be_err());
    expect!("00:00:00".matches_with("1000", &matcher, false)).to(be_err());
    expect!("00:00:00".matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(200, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_err());

    let matcher = MatchingRule::Time("mm:ss".into());
    expect!("100".matches_with("14:01:01", &matcher, false)).to(be_err());
    expect!("100".matches_with("61:01", &matcher, false)).to(be_err());

    let matcher = MatchingRule::Time("ss:mm:HH".into());
    expect!("100".matches_with("05:10:14", &matcher, false)).to(be_ok());

    let matcher = MatchingRule::Time("".into());
    expect!("100".matches_with("14:00:00", &matcher, false)).to(be_ok());
  }

  #[test]
  #[cfg(feature = "datetime")]
  fn date_matcher_test() {
    let matcher = MatchingRule::Date("yyyy-MM-dd".into());
    let matcher2 = MatchingRule::Date("MM/dd/yyyy".into());

    expect!("100".matches_with("2001-10-01", &matcher, false)).to(be_ok());
    expect!("100".matches_with("01/14/2001", &matcher2, false)).to(be_ok());
    expect!("100".matches_with("01-13-01", &matcher, false)).to(be_err());
    expect!("100".matches_with("100", &matcher, false)).to(be_err());
    expect!("100".matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".matches_with("1000", &matcher, false)).to(be_err());
    expect!("100".matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(200, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_err());

    let matcher = MatchingRule::Date("".into());
    expect!("".matches_with("2001-10-01", &matcher, false)).to(be_ok());
  }

  #[test]
  fn include_matcher_test() {
    let matcher = MatchingRule::Include("10".into());
    expect!("100".matches_with("100", &matcher, false)).to(be_ok());
    expect!("100".matches_with("10a", &matcher, false)).to(be_ok());
    expect!("100".matches_with("1000", &matcher, false)).to(be_ok());
    expect!("100".matches_with("200", &matcher, false)).to(be_err());
    expect!("100".matches_with(100, &matcher, false)).to(be_ok());
    expect!(100.matches_with(100, &matcher, false)).to(be_ok());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_ok());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
  }

  #[test]
  fn number_matcher_test() {
    let matcher = MatchingRule::Number;
    expect!("100".matches_with("100", &matcher, false)).to(be_ok());
    expect!("100".matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".matches_with("1000", &matcher, false)).to(be_ok());
    expect!("100".matches_with(100, &matcher, false)).to(be_ok());
    expect!(100.matches_with(200, &matcher, false)).to(be_ok());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_ok());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
  }

  #[test]
  fn integer_matcher_test() {
    let matcher = MatchingRule::Integer;
    expect!("100".matches_with("100", &matcher, false)).to(be_ok());
    expect!("100".matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".matches_with("1000", &matcher, false)).to(be_ok());
    expect!("100".matches_with(100, &matcher, false)).to(be_ok());
    expect!(100.matches_with(200, &matcher, false)).to(be_ok());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_err());
  }

  #[test]
  fn decimal_matcher_test() {
    let matcher = MatchingRule::Decimal;
    expect!("100".matches_with("100", &matcher, false)).to(be_ok());
    expect!("100".matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".matches_with("1000", &matcher, false)).to(be_ok());
    expect!("100".matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(200, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_ok());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_ok());
  }

  #[test]
  fn null_matcher_test() {
    let matcher = MatchingRule::Null;
    expect!("100".matches_with("100", &matcher, false)).to(be_err());
    expect!("100".matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".matches_with("1000", &matcher, false)).to(be_err());
    expect!("100".matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(200, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(100.1f64.matches_with(100.2, &matcher, false)).to(be_err());
  }

  #[test]
  fn regex_matcher_supports_crazy_regexes() {
    let matcher = MatchingRule::Regex(
      r"^([\+-]?\d{4}(?!\d{2}\b))((-?)((0[1-9]|1[0-2])(\3([12]\d|0[1-9]|3[01]))?|W([0-4]\d|5[0-2])(-?[1-7])?|(00[1-9]|0[1-9]\d|[12]\d{2}|3([0-5]\d|6[1-6])))?)$"
        .into());
    expect!("100".matches_with("2019-09-27", &matcher, false)).to(be_ok());
  }

  #[test]
  fn boolean_matcher_test() {
    let matcher = MatchingRule::Boolean;
    expect!("100".to_string().matches_with("100", &matcher, false)).to(be_err());
    expect!("100".to_string().matches_with("10a", &matcher, false)).to(be_err());
    expect!("100".to_string().matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!("100".to_string().matches_with("true", &matcher, false)).to(be_ok());
    expect!("100".to_string().matches_with("false", &matcher, false)).to(be_ok());
    expect!(false.matches_with(true, &matcher, false)).to(be_ok());
  }

  #[test]
  fn match_status_code_test() {
    expect!(match_status_code(100, &HttpStatus::Information)).to(be_ok());
    expect!(match_status_code(199, &HttpStatus::Information)).to(be_ok());
    expect!(match_status_code(500, &HttpStatus::Information)).to(be_err());
    expect!(match_status_code(200, &HttpStatus::Success)).to(be_ok());
    expect!(match_status_code(400, &HttpStatus::Success)).to(be_err());
    expect!(match_status_code(301, &HttpStatus::Redirect)).to(be_ok());
    expect!(match_status_code(500, &HttpStatus::Redirect)).to(be_err());
    expect!(match_status_code(404, &HttpStatus::ClientError)).to(be_ok());
    expect!(match_status_code(500, &HttpStatus::ClientError)).to(be_err());
    expect!(match_status_code(503, &HttpStatus::ServerError)).to(be_ok());
    expect!(match_status_code(499, &HttpStatus::ServerError)).to(be_err());
    expect!(match_status_code(200, &HttpStatus::StatusCodes(vec![200, 201, 204]))).to(be_ok());
    expect!(match_status_code(202, &HttpStatus::StatusCodes(vec![200, 201, 204]))).to(be_err());
    expect!(match_status_code(333, &HttpStatus::NonError)).to(be_ok());
    expect!(match_status_code(599, &HttpStatus::NonError)).to(be_err());
    expect!(match_status_code(555, &HttpStatus::Error)).to(be_ok());
    expect!(match_status_code(99, &HttpStatus::Error)).to(be_err());
  }

  #[test]
  fn not_empty_matcher_test() {
    let matcher = MatchingRule::NotEmpty;
    expect!("100".to_string().matches_with("100", &matcher, false)).to(be_ok());
    expect!("100".to_string().matches_with("", &matcher, false)).to(be_err());
    expect!("100".to_string().matches_with(100, &matcher, false)).to(be_err());
    expect!(100.matches_with(100.1, &matcher, false)).to(be_err());
    expect!(vec![100].matches_with(vec![100], &matcher, false)).to(be_ok());
    expect!(vec![100].matches_with(vec![], &matcher, false)).to(be_err());
    expect!(json!([100]).matches_with(&json!([100]), &matcher, false)).to(be_ok());
    expect!(json!([100]).matches_with(&json!([]), &matcher, false)).to(be_err());
    expect!(json!({"num": 100}).matches_with(&json!({"num": 100}), &matcher, false)).to(be_ok());
    expect!(json!({"num": 100}).matches_with(&json!({}), &matcher, false)).to(be_err());
  }

  #[test]
  fn semver_matcher_test() {
    let matcher = MatchingRule::Semver;
    expect!("1.0.0".to_string().matches_with("1.0.0", &matcher, false)).to(be_ok());
    expect!("1.0.0".to_string().matches_with("1", &matcher, false)).to(be_err());
    expect!("1.0.0".to_string().matches_with("1.0.0-beta.1", &matcher, false)).to(be_ok());
    expect!(json!("1.0.0").matches_with(&json!("1.0.0"), &matcher, false)).to(be_ok());
    expect!(json!("1.0.0").matches_with(&json!("1"), &matcher, false)).to(be_err());
  }

  #[test]
  fn content_type_matcher_test() {
    let matcher = MatchingRule::ContentType("text/plain".to_string());
    expect!("plain text".matches_with("plain text", &matcher, false)).to(be_ok());
    expect!("plain text".matches_with("different text", &matcher, false)).to(be_ok());
    expect!("plain text".matches_with(100, &matcher, false)).to(be_err());
    {
      let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
      <note>
        <to>Tove</to>
        <from>Jani</from>
        <heading>Reminder</heading>
        <body>Don't forget me this weekend!</body>
      </note>"#;
      expect!("plain text".matches_with(xml, &matcher, false)).to(be_err());
    }
  }
}
