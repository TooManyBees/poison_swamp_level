use compact_str::CompactString;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;

#[derive(Debug, Default)]
pub struct NamedHashMap {
    pub inner: HashMap<MetricKey, AtomicU64>,
    pub name: CompactString,
    pub desc: &'static str,
}

impl std::ops::Deref for NamedHashMap {
    type Target = HashMap<MetricKey, AtomicU64>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for NamedHashMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl NamedHashMap {
    pub fn new(name: &'static str, desc: &'static str) -> Self {
        NamedHashMap {
            inner: HashMap::new(),
            name: CompactString::const_new(name),
            desc,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetricLabel(pub CompactString, pub CompactString);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetricKey {
    pub name: CompactString,
    pub labels: Vec<MetricLabel>,
}
