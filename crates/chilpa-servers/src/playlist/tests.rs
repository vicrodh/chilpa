use super::*;

/// The list after the moves, and how many there were. Each move lands
/// right after the entry it names, as a server that moves by "after" does.
fn moved(order: &[usize]) -> (Vec<usize>, usize) {
    let mut list: Vec<usize> = (0..order.len()).collect();
    let steps = moves(order);
    for step in &steps {
        let from = list.iter().position(|&x| x == step.item).unwrap();
        list.remove(from);
        let after = step.after.map_or(0, |a| list.iter().position(|&x| x == a).unwrap() + 1);
        assert_eq!(after, step.to, "the place it goes to is right after the one it names");
        list.insert(step.to, step.item);
    }
    (list, steps.len())
}

fn every_order(rest: Vec<usize>, taken: Vec<usize>, out: &mut Vec<Vec<usize>>) {
    if rest.is_empty() {
        out.push(taken);
        return;
    }
    for i in 0..rest.len() {
        let mut left = rest.clone();
        let next = left.remove(i);
        let mut chosen = taken.clone();
        chosen.push(next);
        every_order(left, chosen, out);
    }
}

#[test]
fn a_new_order_is_reached_with_the_fewest_single_moves() {
    assert_eq!(moved(&[0, 1, 2, 3]), (vec![0, 1, 2, 3], 0), "the same order moves nothing");
    assert_eq!(moved(&[3, 0, 1, 2]), (vec![3, 0, 1, 2], 1), "the last one to the top is one move");
    assert_eq!(moved(&[1, 2, 3, 0]), (vec![1, 2, 3, 0], 1), "the first one to the end is one move");
    assert_eq!(moved(&[0, 2, 1, 3]), (vec![0, 2, 1, 3], 1));
    assert_eq!(moved(&[3, 2, 1, 0]), (vec![3, 2, 1, 0], 3), "turned around, all but one move");
    let mut orders = Vec::new();
    every_order((0..5).collect(), Vec::new(), &mut orders);
    assert_eq!(orders.len(), 120);
    for order in orders {
        let (list, steps) = moved(&order);
        assert_eq!(list, order, "every order of five is reached");
        assert!(steps < 5, "{order:?} took {steps}");
    }
}

#[test]
fn an_order_puts_each_entry_somewhere_once() {
    assert!(is_order(&[1, 0, 2], 3));
    assert!(is_order(&[], 0));
    assert!(!is_order(&[0, 0, 1], 3), "one twice");
    assert!(!is_order(&[0, 1], 3), "one left out");
    assert!(!is_order(&[0, 3, 1], 3), "one that is not there");
}

#[test]
fn a_playlist_s_tracks_leave_out_what_is_not_a_track() {
    let read = PlaylistTracks {
        entries: vec![Entry { id: "a".into(), entry: "1".into(), track: true }, Entry { id: "v".into(), entry: "2".into(), track: false }, Entry { id: "a".into(), entry: "3".into(), track: true }],
        whole: true,
    };
    assert_eq!(read.tracks(), ["a", "a"], "with its repeats");
    assert_eq!(read.skipped(), 1);
}
