use crate::cards::Card;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HandRank {
    JacksOrBetter,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
    RoyalFlush,
}

impl HandRank {
    /// Paytable order, top row first.
    pub const ALL_DESC: [HandRank; 9] = [
        HandRank::RoyalFlush,
        HandRank::StraightFlush,
        HandRank::FourOfAKind,
        HandRank::FullHouse,
        HandRank::Flush,
        HandRank::Straight,
        HandRank::ThreeOfAKind,
        HandRank::TwoPair,
        HandRank::JacksOrBetter,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HandRank::RoyalFlush => "Royal Flush",
            HandRank::StraightFlush => "Straight Flush",
            HandRank::FourOfAKind => "Four of a Kind",
            HandRank::FullHouse => "Full House",
            HandRank::Flush => "Flush",
            HandRank::Straight => "Straight",
            HandRank::ThreeOfAKind => "Three of a Kind",
            HandRank::TwoPair => "Two Pair",
            HandRank::JacksOrBetter => "Jacks or Better",
        }
    }

    /// 9/6 Jacks or Better payout in coins for a bet of `coins` (1..=5).
    pub fn payout(self, coins: u8) -> u32 {
        if self == HandRank::RoyalFlush && coins == 5 {
            return 4000;
        }
        let base: u32 = match self {
            HandRank::JacksOrBetter => 1,
            HandRank::TwoPair => 2,
            HandRank::ThreeOfAKind => 3,
            HandRank::Straight => 4,
            HandRank::Flush => 6,
            HandRank::FullHouse => 9,
            HandRank::FourOfAKind => 25,
            HandRank::StraightFlush => 50,
            HandRank::RoyalFlush => 250,
        };
        base * coins as u32
    }
}

const WHEEL: u16 = (1 << 14) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5);
const ROYAL: u16 = 0b11111 << 10;

pub fn evaluate(hand: &[Card; 5]) -> Option<HandRank> {
    let mut counts = [0u8; 15];
    let mut mask: u16 = 0;
    for c in hand {
        counts[c.rank as usize] += 1;
        mask |= 1 << c.rank;
    }
    let flush = hand.iter().all(|c| c.suit == hand[0].suit);
    let straight =
        mask.count_ones() == 5 && ((mask >> mask.trailing_zeros()) == 0b11111 || mask == WHEEL);

    if straight && flush {
        return Some(if mask == ROYAL {
            HandRank::RoyalFlush
        } else {
            HandRank::StraightFlush
        });
    }

    let mut pairs = 0;
    let mut high_pair = false;
    let mut trips = false;
    let mut quads = false;
    for (rank, &n) in counts.iter().enumerate().skip(2) {
        match n {
            4 => quads = true,
            3 => trips = true,
            2 => {
                pairs += 1;
                if rank >= 11 {
                    high_pair = true;
                }
            }
            _ => {}
        }
    }

    if quads {
        Some(HandRank::FourOfAKind)
    } else if trips && pairs == 1 {
        Some(HandRank::FullHouse)
    } else if flush {
        Some(HandRank::Flush)
    } else if straight {
        Some(HandRank::Straight)
    } else if trips {
        Some(HandRank::ThreeOfAKind)
    } else if pairs == 2 {
        Some(HandRank::TwoPair)
    } else if high_pair {
        Some(HandRank::JacksOrBetter)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::hand;

    #[test]
    fn categories() {
        assert_eq!(
            evaluate(&hand("As Ks Qs Js Ts")),
            Some(HandRank::RoyalFlush)
        );
        assert_eq!(
            evaluate(&hand("9h 8h 7h 6h 5h")),
            Some(HandRank::StraightFlush)
        );
        assert_eq!(
            evaluate(&hand("Ah 2h 3h 4h 5h")),
            Some(HandRank::StraightFlush)
        );
        assert_eq!(
            evaluate(&hand("7c 7d 7h 7s Ks")),
            Some(HandRank::FourOfAKind)
        );
        assert_eq!(evaluate(&hand("7c 7d 7h Ks Kd")), Some(HandRank::FullHouse));
        assert_eq!(evaluate(&hand("2d 9d Kd 4d 7d")), Some(HandRank::Flush));
        assert_eq!(evaluate(&hand("9h 8c 7d 6s 5h")), Some(HandRank::Straight));
        assert_eq!(evaluate(&hand("Ah 2c 3d 4s 5h")), Some(HandRank::Straight));
        assert_eq!(evaluate(&hand("Ah Kc Qd Js 10h")), Some(HandRank::Straight));
        assert_eq!(
            evaluate(&hand("7c 7d 7h Ks 2d")),
            Some(HandRank::ThreeOfAKind)
        );
        assert_eq!(evaluate(&hand("7c 7d Kh Ks 2d")), Some(HandRank::TwoPair));
        assert_eq!(
            evaluate(&hand("Jc Jd 3h 8s 2d")),
            Some(HandRank::JacksOrBetter)
        );
        assert_eq!(
            evaluate(&hand("Ac Ad 3h 8s 2d")),
            Some(HandRank::JacksOrBetter)
        );
    }

    #[test]
    fn non_paying_hands() {
        assert_eq!(evaluate(&hand("10c 10d 3h 8s 2d")), None);
        assert_eq!(evaluate(&hand("Ac Kd Qh Js 2d")), None);
        // K-A-2-3-4 does not wrap around.
        assert_eq!(evaluate(&hand("Kc Ad 2h 3s 4d")), None);
    }

    #[test]
    fn paytable_9_6() {
        let rows: [(HandRank, [u32; 5]); 9] = [
            (HandRank::RoyalFlush, [250, 500, 750, 1000, 4000]),
            (HandRank::StraightFlush, [50, 100, 150, 200, 250]),
            (HandRank::FourOfAKind, [25, 50, 75, 100, 125]),
            (HandRank::FullHouse, [9, 18, 27, 36, 45]),
            (HandRank::Flush, [6, 12, 18, 24, 30]),
            (HandRank::Straight, [4, 8, 12, 16, 20]),
            (HandRank::ThreeOfAKind, [3, 6, 9, 12, 15]),
            (HandRank::TwoPair, [2, 4, 6, 8, 10]),
            (HandRank::JacksOrBetter, [1, 2, 3, 4, 5]),
        ];
        for (rank, pays) in rows {
            for (i, p) in pays.iter().enumerate() {
                assert_eq!(rank.payout(i as u8 + 1), *p, "{:?} x{}", rank, i + 1);
            }
        }
    }
}
