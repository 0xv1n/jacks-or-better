use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde::{Deserialize, Serialize};

use crate::cards::{Card, Deck};
use crate::hand::{HandRank, evaluate};
use crate::strategy::{self, Analysis};

/// Denominations in cents.
pub const DENOMS: [u32; 5] = [1, 5, 25, 50, 100];
pub const START_BANKROLL: u64 = 10_000;
pub const MAX_COINS: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Dealt,
}

#[derive(Clone, Debug)]
pub struct Feedback {
    pub optimal: bool,
    pub dealt: [Card; 5],
    /// Bit i set = card i was held.
    pub player_mask: u8,
    pub player_ev: f64,
    /// The best hold; the player's own when it ties for best.
    pub best_mask: u8,
    pub best_ev: f64,
}

/// What survives a restart.
#[derive(Serialize, Deserialize)]
pub struct Saved {
    pub bankroll: u64,
    pub denom: u32,
    pub coins: u8,
    pub correct: u32,
    pub decisions: u32,
    pub streak: u32,
}

pub struct Game {
    pub bankroll: u64,
    pub denom: u32,
    pub coins: u8,
    pub phase: Phase,
    pub hand: Option<[Card; 5]>,
    pub held: [bool; 5],
    /// Incremented each time a card slot receives a new card (drives flip animation).
    pub card_gen: [u32; 5],
    pub result: Option<HandRank>,
    /// Last hand's win in cents and in coins, frozen at settle time.
    pub last_win: u64,
    pub last_win_coins: u32,
    pub feedback: Option<Feedback>,
    pub correct: u32,
    pub decisions: u32,
    /// Consecutive optimal holds.
    pub streak: u32,
    deck: Deck,
    analysis: Option<Receiver<Analysis>>,
}

impl Game {
    pub fn new() -> Game {
        Game {
            bankroll: START_BANKROLL,
            denom: 25,
            coins: MAX_COINS,
            phase: Phase::Ready,
            hand: None,
            held: [false; 5],
            card_gen: [0; 5],
            result: None,
            last_win: 0,
            last_win_coins: 0,
            feedback: None,
            correct: 0,
            decisions: 0,
            streak: 0,
            deck: Deck::shuffled(),
            analysis: None,
        }
    }

    pub fn restore(s: Saved) -> Game {
        let mut g = Game::new();
        g.bankroll = s.bankroll;
        if DENOMS.contains(&s.denom) {
            g.denom = s.denom;
        }
        if (1..=MAX_COINS).contains(&s.coins) {
            g.coins = s.coins;
        }
        g.decisions = s.decisions;
        g.correct = s.correct.min(s.decisions);
        g.streak = s.streak.min(g.correct);
        g
    }

    pub fn saved(&self) -> Saved {
        // A hand still waiting on DRAW is abandoned, so its bet is returned.
        let refund = if self.phase == Phase::Dealt {
            self.bet_cents()
        } else {
            0
        };
        Saved {
            bankroll: self.bankroll + refund,
            denom: self.denom,
            coins: self.coins,
            correct: self.correct,
            decisions: self.decisions,
            streak: self.streak,
        }
    }

    pub fn bet_cents(&self) -> u64 {
        self.coins as u64 * self.denom as u64
    }

    pub fn can_deal(&self) -> bool {
        self.phase == Phase::Ready && self.bankroll >= self.bet_cents()
    }

    /// Broke at every denomination, not just the current one.
    pub fn is_bust(&self) -> bool {
        self.phase == Phase::Ready && self.bankroll < DENOMS[0] as u64
    }

    pub fn set_denom(&mut self, denom: u32) {
        if self.phase == Phase::Ready && DENOMS.contains(&denom) {
            self.denom = denom;
        }
    }

    pub fn bet_one(&mut self) {
        if self.phase == Phase::Ready {
            self.coins = self.coins % MAX_COINS + 1;
        }
    }

    pub fn can_bet_max(&self) -> bool {
        self.phase == Phase::Ready && self.bankroll >= MAX_COINS as u64 * self.denom as u64
    }

    pub fn bet_max(&mut self) {
        if self.can_bet_max() {
            self.coins = MAX_COINS;
            self.deal();
        }
    }

    pub fn deal(&mut self) {
        if !self.can_deal() {
            return;
        }
        self.bankroll -= self.bet_cents();
        self.deck = Deck::shuffled();
        let hand: [Card; 5] = std::array::from_fn(|_| self.deck.deal());
        self.hand = Some(hand);
        self.held = [false; 5];
        for g in &mut self.card_gen {
            *g += 1;
        }
        self.result = None;
        self.last_win = 0;
        self.last_win_coins = 0;
        self.feedback = None;
        self.phase = Phase::Dealt;

        let (tx, rx) = mpsc::channel();
        let coins = self.coins;
        thread::spawn(move || {
            let _ = tx.send(strategy::analyze(&hand, coins));
        });
        self.analysis = Some(rx);
    }

    pub fn toggle_hold(&mut self, i: usize) {
        if self.phase == Phase::Dealt {
            self.held[i] = !self.held[i];
        }
    }

    pub fn draw(&mut self) {
        if self.phase != Phase::Dealt {
            return;
        }
        let mut hand = self.hand.expect("dealt phase has a hand");
        for (i, card) in hand.iter_mut().enumerate() {
            if !self.held[i] {
                *card = self.deck.deal();
                self.card_gen[i] += 1;
            }
        }
        self.hand = Some(hand);
        self.settle(evaluate(&hand));
        if let Some(rx) = self.analysis.take()
            && let Ok(a) = rx.recv()
        {
            self.grade(&a);
        }
        self.phase = Phase::Ready;
    }

    pub fn reset(&mut self) {
        if self.phase == Phase::Ready {
            self.bankroll = START_BANKROLL;
        }
    }

    pub fn reset_stats(&mut self) {
        self.correct = 0;
        self.decisions = 0;
        self.streak = 0;
    }

    fn settle(&mut self, rank: Option<HandRank>) {
        self.result = rank;
        self.last_win_coins = rank.map_or(0, |r| r.payout(self.coins));
        self.last_win = self.last_win_coins as u64 * self.denom as u64;
        self.bankroll += self.last_win;
    }

    fn grade(&mut self, a: &Analysis) {
        let mask = strategy::mask_of(&self.held);
        let best = a.best();
        let optimal = a.is_optimal(mask);
        self.decisions += 1;
        if optimal {
            self.correct += 1;
            self.streak += 1;
        } else {
            self.streak = 0;
        }
        self.feedback = Some(Feedback {
            optimal,
            dealt: a.hand,
            player_mask: mask,
            player_ev: a.ev_of(mask),
            best_mask: if optimal { mask } else { best.mask },
            best_ev: best.ev,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_100_dollars_at_quarter_denom() {
        let g = Game::new();
        assert_eq!(g.bankroll, 10_000);
        assert_eq!(g.denom, 25);
        assert_eq!(g.coins, 5);
        assert_eq!(g.bankroll / g.denom as u64, 400);
        assert_eq!(g.bet_cents(), 125);
    }

    #[test]
    fn deal_deducts_bet_and_locks_denom() {
        let mut g = Game::new();
        g.deal();
        assert_eq!(g.phase, Phase::Dealt);
        assert_eq!(g.bankroll, 10_000 - 125);
        g.set_denom(100);
        assert_eq!(g.denom, 25);
        g.bet_one();
        assert_eq!(g.coins, 5);
        g.draw();
        assert_eq!(g.phase, Phase::Ready);
        assert_eq!(g.decisions, 1);
        g.set_denom(100);
        assert_eq!(g.denom, 100);
    }

    #[test]
    fn cannot_deal_without_funds() {
        let mut g = Game::new();
        g.bankroll = 100;
        assert!(!g.can_deal());
        g.deal();
        assert_eq!(g.phase, Phase::Ready);
        g.set_denom(5);
        assert!(g.can_deal());
        g.bankroll = 3;
        assert!(!g.can_deal());
        assert!(!g.is_bust(), "a smaller denomination is still playable");
        g.bankroll = 0;
        assert!(g.is_bust());
        g.reset();
        assert_eq!(g.bankroll, START_BANKROLL);
    }

    #[test]
    fn bet_one_cycles() {
        let mut g = Game::new();
        g.bet_one();
        assert_eq!(g.coins, 1);
        for _ in 0..4 {
            g.bet_one();
        }
        assert_eq!(g.coins, 5);
    }

    #[test]
    fn settle_pays_by_denom_and_freezes_win() {
        let mut g = Game::new();
        g.settle(Some(HandRank::FullHouse));
        assert_eq!(g.last_win, 45 * 25);
        assert_eq!(g.last_win_coins, 45);
        assert_eq!(g.bankroll, 10_000 + 1125);
        g.bet_one();
        g.set_denom(1);
        assert_eq!(g.last_win_coins, 45);
        assert_eq!(g.last_win, 1125);
        g.settle(None);
        assert_eq!(g.last_win, 0);
        assert_eq!(g.last_win_coins, 0);
        assert_eq!(g.result, None);
    }

    #[test]
    fn streak_counts_consecutive_optimal_holds() {
        let mut g = Game::new();
        let a = strategy::analyze(&crate::cards::hand("7c 7d 7h Ks Kd"), 5);
        g.held = [true; 5];
        g.grade(&a);
        g.grade(&a);
        assert_eq!((g.correct, g.decisions, g.streak), (2, 2, 2));
        g.held = [false; 5];
        g.grade(&a);
        assert_eq!((g.correct, g.decisions, g.streak), (2, 3, 0));
        let fb = g.feedback.as_ref().unwrap();
        assert!(!fb.optimal);
        assert_eq!((fb.player_mask, fb.best_mask), (0, 0b11111));
        g.held = [true; 5];
        g.grade(&a);
        g.reset_stats();
        assert_eq!((g.correct, g.decisions, g.streak), (0, 0, 0));
    }

    #[test]
    fn save_refunds_unfinished_hand_and_restore_validates() {
        let mut g = Game::new();
        g.deal();
        let s = g.saved();
        assert_eq!(s.bankroll, START_BANKROLL);
        let g = Game::restore(Saved {
            bankroll: 42,
            denom: 3,
            coins: 9,
            correct: 7,
            decisions: 5,
            streak: 9,
        });
        assert_eq!((g.bankroll, g.denom, g.coins), (42, 25, MAX_COINS));
        assert_eq!((g.correct, g.decisions, g.streak), (5, 5, 5));
        assert_eq!(g.phase, Phase::Ready);
    }

    #[test]
    fn bet_max_requires_five_coins_of_bankroll() {
        let mut g = Game::new();
        g.bet_one();
        g.bankroll = 124;
        assert!(!g.can_bet_max());
        g.bet_max();
        assert_eq!(g.coins, 1);
        assert_eq!(g.phase, Phase::Ready);
        g.bankroll = 125;
        g.bet_max();
        assert_eq!(g.coins, 5);
        assert_eq!(g.phase, Phase::Dealt);
        assert_eq!(g.bankroll, 0);
    }
}
