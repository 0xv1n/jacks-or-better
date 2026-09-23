use crate::cards::{Card, full_deck};
use crate::hand::evaluate;

#[derive(Clone, Copy, Debug)]
pub struct HoldEv {
    /// Bit i set = card i is held.
    pub mask: u8,
    /// Expected return per coin bet.
    pub ev: f64,
}

#[derive(Clone, Debug)]
pub struct Analysis {
    pub hand: [Card; 5],
    /// All 32 holds, best first.
    pub holds: Vec<HoldEv>,
}

impl Analysis {
    pub fn best(&self) -> HoldEv {
        self.holds[0]
    }

    pub fn ev_of(&self, mask: u8) -> f64 {
        self.holds
            .iter()
            .find(|h| h.mask == mask)
            .map(|h| h.ev)
            .expect("mask out of range")
    }

    pub fn is_optimal(&self, mask: u8) -> bool {
        (self.ev_of(mask) - self.best().ev).abs() < 1e-9
    }
}

pub fn mask_of(held: &[bool; 5]) -> u8 {
    held.iter()
        .enumerate()
        .fold(0, |m, (i, &h)| if h { m | (1 << i) } else { m })
}

/// Exact expected value of every possible hold by enumerating all draws
/// from the 47 unseen cards, using the paytable for the number of coins bet
/// (the royal pays 800 per coin only at five coins).
pub fn analyze(hand: &[Card; 5], coins: u8) -> Analysis {
    let remaining: Vec<Card> = full_deck()
        .into_iter()
        .filter(|c| !hand.contains(c))
        .collect();
    let mut holds = Vec::with_capacity(32);
    for mask in 0u8..32 {
        let open: Vec<usize> = (0..5).filter(|i| mask & (1 << i) == 0).collect();
        let mut trial = *hand;
        let mut total: u64 = 0;
        let mut n: u64 = 0;
        for_each_combination(remaining.len(), open.len(), |idx| {
            for (&slot, &j) in open.iter().zip(idx) {
                trial[slot] = remaining[j];
            }
            if let Some(r) = evaluate(&trial) {
                total += r.payout(coins) as u64;
            }
            n += 1;
        });
        holds.push(HoldEv {
            mask,
            ev: total as f64 / (n as f64 * coins as f64),
        });
    }
    holds.sort_by(|a, b| b.ev.partial_cmp(&a.ev).unwrap());
    Analysis { hand: *hand, holds }
}

fn for_each_combination(n: usize, k: usize, mut f: impl FnMut(&[usize])) {
    if k == 0 {
        f(&[]);
        return;
    }
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        f(&idx);
        let mut i = k - 1;
        while idx[i] == n - k + i {
            if i == 0 {
                return;
            }
            i -= 1;
        }
        idx[i] += 1;
        for j in i + 1..k {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::hand;

    #[test]
    fn combination_count() {
        let mut n = 0;
        for_each_combination(47, 5, |_| n += 1);
        assert_eq!(n, 1_533_939);
        n = 0;
        for_each_combination(47, 0, |_| n += 1);
        assert_eq!(n, 1);
    }

    #[test]
    fn made_hand_ev_equals_payout() {
        let a = analyze(&hand("7c 7d 7h Ks Kd"), 5);
        assert!((a.ev_of(31) - 9.0).abs() < 1e-12);
        let a = analyze(&hand("As Ks Qs Js Ts"), 5);
        assert_eq!(a.best().mask, 31);
        assert!((a.best().ev - 800.0).abs() < 1e-12);
    }

    #[test]
    fn four_to_royal_beats_made_flush() {
        let a = analyze(&hand("Ts Js Qs Ks 5s"), 5);
        assert_eq!(a.best().mask, 0b01111);
        assert!(a.best().ev > 6.0);
    }

    #[test]
    fn four_flush_beats_low_pair() {
        let a = analyze(&hand("2s 2h 7s 9s Ks"), 5);
        assert_eq!(a.best().mask, 0b11101);
        assert!(!a.is_optimal(0b00011));
    }

    #[test]
    fn low_pair_beats_open_straight_draw() {
        let a = analyze(&hand("5s 5h 6d 7c 8s"), 5);
        assert_eq!(a.best().mask, 0b00011);
    }

    #[test]
    fn junk_holds_single_high_card() {
        let a = analyze(&hand("2c 5d 8h 9s Kd"), 5);
        assert_eq!(a.best().mask, 0b10000);
        assert!(a.ev_of(0b10000) > a.ev_of(0));
    }

    #[test]
    fn short_coin_strategy_uses_short_coin_paytable() {
        // K♠ Q♠ 10♠ 7♠ 2♦: 3 to a royal (hold K Q 10) vs 4 to a flush (hold all spades).
        let three_royal = 0b00111;
        let four_flush = 0b01111;
        let max = analyze(&hand("Ks Qs Ts 7s 2d"), 5);
        assert_eq!(max.best().mask, three_royal);
        let one = analyze(&hand("Ks Qs Ts 7s 2d"), 1);
        assert_eq!(one.best().mask, four_flush);
    }

    #[test]
    fn ties_count_as_optimal() {
        // Four of a kind: keeping or discarding the kicker is identical.
        let a = analyze(&hand("7c 7d 7h 7s Ks"), 5);
        assert!(a.is_optimal(0b01111));
        assert!(a.is_optimal(0b11111));
        assert!(!a.is_optimal(0b00111));
    }

    #[test]
    fn mask_helpers() {
        assert_eq!(mask_of(&[true, false, false, false, true]), 0b10001);
    }
}
