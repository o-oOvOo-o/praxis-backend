pub(super) fn filter_items<T>(
    items: &[T],
    previous: Option<usize>,
    filter: &str,
    name: impl Fn(&T) -> &str,
    score: impl Fn(&T) -> Option<i32>,
) -> (Vec<usize>, Option<usize>) {
    let indices = if filter.is_empty() {
        (0..items.len()).collect::<Vec<_>>()
    } else {
        let mut matches = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| score(item).map(|score| (index, score)))
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| name(&items[a.0]).cmp(name(&items[b.0])))
        });
        matches
            .into_iter()
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    let selected = previous
        .and_then(|index| indices.iter().position(|candidate| *candidate == index))
        .or_else(|| (!indices.is_empty()).then_some(0));
    (indices, selected)
}

pub(super) fn checkbox_label(name: &str, selected: bool, enabled: bool) -> String {
    let prefix = if selected { '›' } else { ' ' };
    let marker = if enabled { 'x' } else { ' ' };
    format!("{prefix} [{marker}] {name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtering_preserves_identity_and_orders_ties_by_name() {
        let items = ["beta", "alpha", "hidden"];
        let (indices, selected) = filter_items(
            &items,
            Some(0),
            "a",
            |item| *item,
            |item| item.contains('a').then_some(1),
        );
        assert_eq!((indices, selected), (vec![1, 0], Some(1)));
        assert_eq!(
            filter_items(
                &items,
                Some(2),
                "a",
                |item| *item,
                |item| item.contains('a').then_some(1)
            )
            .1,
            Some(0)
        );
        assert_eq!(
            filter_items(&items, Some(0), "none", |item| *item, |_| None),
            (vec![], None)
        );
    }
    #[test]
    fn checkbox_rendering_is_stable() {
        assert_eq!(checkbox_label("Skill", true, true), "› [x] Skill");
        assert_eq!(checkbox_label("Skill", false, false), "  [ ] Skill");
    }
}
