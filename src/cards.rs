#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub fn is_red(self) -> bool {
        matches!(self, Suit::Diamonds | Suit::Hearts)
    }

    pub fn name(self) -> &'static str {
        match self {
            Suit::Clubs => "clubs",
            Suit::Diamonds => "diamonds",
            Suit::Hearts => "hearts",
            Suit::Spades => "spades",
        }
    }
}

/// A playing card. `rank` is 2..=14 where 11=J, 12=Q, 13=K, 14=A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub rank: u8,
    pub suit: Suit,
}

impl Card {
    pub fn rank_label(self) -> &'static str {
        match self.rank {
            2 => "2",
            3 => "3",
            4 => "4",
            5 => "5",
            6 => "6",
            7 => "7",
            8 => "8",
            9 => "9",
            10 => "10",
            11 => "J",
            12 => "Q",
            13 => "K",
            14 => "A",
            _ => "?",
        }
    }

    /// Spoken name, e.g. "Queen of hearts", for screen readers.
    pub fn name(self) -> String {
        let rank = match self.rank {
            11 => "Jack",
            12 => "Queen",
            13 => "King",
            14 => "Ace",
            _ => self.rank_label(),
        };
        format!("{rank} of {}", self.suit.name())
    }

    /// Parse "Js", "10h", "Ad" style notation. Used by tests.
    #[cfg(test)]
    pub fn parse(s: &str) -> Card {
        let (r, su) = s.split_at(s.len() - 1);
        let rank = match r {
            "J" => 11,
            "Q" => 12,
            "K" => 13,
            "A" => 14,
            "T" => 10,
            n => n.parse().expect("bad rank"),
        };
        let suit = match su {
            "c" => Suit::Clubs,
            "d" => Suit::Diamonds,
            "h" => Suit::Hearts,
            "s" => Suit::Spades,
            _ => panic!("bad suit"),
        };
        Card { rank, suit }
    }
}

pub fn full_deck() -> Vec<Card> {
    let mut v = Vec::with_capacity(52);
    for suit in Suit::ALL {
        for rank in 2..=14 {
            v.push(Card { rank, suit });
        }
    }
    v
}

pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn shuffled() -> Deck {
        use rand::seq::SliceRandom;
        let mut cards = full_deck();
        cards.shuffle(&mut rand::rng());
        Deck { cards }
    }

    pub fn deal(&mut self) -> Card {
        self.cards.pop().expect("deck exhausted")
    }
}

#[cfg(test)]
pub fn hand(s: &str) -> [Card; 5] {
    let v: Vec<Card> = s.split_whitespace().map(Card::parse).collect();
    v.try_into().expect("need 5 cards")
}
