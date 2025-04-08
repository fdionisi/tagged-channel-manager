use std::ops::AddAssign;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ChannelId(u64);

impl AddAssign<u64> for ChannelId {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

impl Copy for ChannelId {}

impl From<u64> for ChannelId {
    fn from(id: u64) -> Self {
        ChannelId(id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn it_initiate() {
        let id = ChannelId(123);
        assert_eq!(id.0, 123);
    }

    #[test]
    fn it_can_compare() {
        assert_eq!(ChannelId(10), ChannelId(10))
    }

    #[test]
    fn it_can_be_copied_over_clone() {
        fn test(id: ChannelId) -> ChannelId {
            id
        }

        let id = ChannelId(123);
        let copied_id = test(id);

        assert_eq!(id, copied_id);
    }

    #[test]
    fn it_can_be_used_for_hash_map_index() {
        let mut map = HashMap::<ChannelId, String>::default();
        let index = ChannelId(0);

        map.insert(index, "value".into());
        assert_eq!(map.get(&index), Some(&"value".into()));
    }

    #[test]
    fn it_initialize_from_u64() {
        let id: ChannelId = 64.into();

        assert_eq!(id, ChannelId(64))
    }

    #[test]
    fn it_creates_default() {
        let id: ChannelId = Default::default();

        assert_eq!(id, ChannelId(0))
    }

    #[test]
    fn it_can_add_u64() {
        let mut id = ChannelId(0);

        id += 1;

        assert_eq!(id, ChannelId(1))
    }
}
