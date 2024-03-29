#![allow(clippy::result_unit_err)]
//! This is the evaluator for checking what condition a certain move creates.
//!
//! It is either an illegal move, (i.e) black makes a three-three, four-four or an overline. Or it
//! is an overline (win for white), five (win for black and white), four (can become a five) or an three
//! (can become a four). A four and a three can also be either in two states, open or closed. An
//! open three will always be able to become a four, an open four will always become an five.
//! These states are easy to check for white, but it becomes trickier when the move is done by
//! black. Black cannot place a stone that actively is part of any of the illegal moves, but a
//! three-four can become a four-four (e.g).
//!
//! # Implementation.
//!

use super::{BoardArr, Point, Stone};

use std::collections::{BTreeMap, BTreeSet};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum Direction {
    /// Horizontal line `|`
    Horizontal,
    /// Vertical line `-`
    Vertical,
    /// Diagonal line `/` or `\`
    Diagonal {
        /// Top or bottom. true = `/`, top = `\`
        bottom: bool,
    },
}
impl Direction {
    pub const fn directions() -> [Direction; 4] {
        [
            Direction::Horizontal,
            Direction::Vertical,
            Direction::Diagonal { bottom: false },
            Direction::Diagonal { bottom: true },
        ]
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
/// A condition of stones on a board. The place a stone could be placed to create a certain condition.
pub enum RenjuCondition {
    /// A unbroken row of three stones which with one move can become a straight four.
    ///
    /// ```text
    /// .......
    /// ..XXX..
    /// .......
    /// ```
    ///
    /// # Notes
    ///
    /// ```text
    /// ....
    /// OXXX.
    /// .....
    /// ```
    ///
    /// is not a UnbrokenThree
    UnbrokenThree {
        /// Direction of the row.
        direction: Direction,
        /// The stones in the row.
        stones: [Point; 3],
        /// The open point in the row. A move on this point will create a three and that three can become a straight four.
        place: [Point; 1],
    },
    /// A broken row of three stones which with one move can become a straight four
    ///
    /// ```text
    /// ......
    /// .X.XX.
    /// ..^...
    /// ```
    ///
    /// The `^` is the break point.
    ///
    /// # Notes
    ///
    /// This is a special case of the unbroken three, where the break point is in the middle.
    ///
    /// The following is not a BrokenThree
    ///
    /// ```text
    /// ......
    /// OX.XX.
    /// ..^...
    /// ```
    BrokenThree {
        /// Direction of the row.
        direction: Direction,
        /// The stones in the row.
        stones: [Point; 4],
        /// The break point in the row. A move on this point will create a three and that three can become a straight four.
        place: [Point; 1],
    },
    // /// A closed row of three stones which with one move can become four
    // ClosedThree {
    //     direction: Direction,
    //     stones: [Point; 3],
    //     open: [Point; 1],
    //     closed: [Point; 1],
    // },
    /// A row of four stones with which two ways exists that can in one move can become five.
    StraightFour {
        /// Direction of the row.
        direction: Direction,
        /// The stones in the row.
        stones: [Point; 4],
        /// The open points in the row. A move on these points will create a five.
        place: [Point; 1],
    },
    /// A row of four stones with which one way can in one move can become five.
    ///
    /// Also known as a half open four
    ClosedFour {
        direction: Direction,
        stones: [Point; 4],
        place: [Point; 1],
    },
    /// A row of four stones including a break with which one way can in one move can become five.
    BrokenFour {
        direction: Direction,
        stones: [Point; 5],
        place: [Point; 1],
    },
    /// A row of five stones. Win condition for the player.
    Five {
        /// Direction of the row.
        direction: Direction,
        /// The stones in the row.
        stones: [Point; 5],
        place: [Point; 1],
    },
}

impl RenjuCondition {
    pub fn stones(&self) -> &[Point] {
        match self {
            RenjuCondition::UnbrokenThree { stones, .. } => stones.as_slice(),
            RenjuCondition::BrokenThree { stones, .. } => stones.as_slice(),
            RenjuCondition::StraightFour { stones, .. } => stones.as_slice(),
            RenjuCondition::ClosedFour { stones, .. } => stones.as_slice(),
            RenjuCondition::BrokenFour { stones, .. } => stones.as_slice(),
            RenjuCondition::Five { stones, .. } => stones.as_slice(),
        }
    }

    pub fn place(&self) -> &Point {
        match self {
            RenjuCondition::UnbrokenThree { place, .. } => &place[0],
            RenjuCondition::BrokenThree { place, .. } => &place[0],
            RenjuCondition::StraightFour { place, .. } => &place[0],
            RenjuCondition::ClosedFour { place, .. } => &place[0],
            RenjuCondition::BrokenFour { place, .. } => &place[0],
            RenjuCondition::Five { place, .. } => &place[0],
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Default)]
pub struct RenjuConditions {
    pub conditions: BTreeSet<RenjuCondition>,
    pub forbidden: BTreeSet<Point>,
    pub threes: BTreeSet<(RenjuCondition, Point)>,
}

impl BoardArr {
    /// A condition is a place where a stone could be placed to create a certain condition.
    #[tracing::instrument(skip(self, stone, only_including))]
    pub fn renju_conditions(
        &self,
        stone: Stone,
        only_including: Option<&[Point]>,
    ) -> RenjuConditions {
        static NULL_POINT: Point = Point {
            x: 0,
            y: 0,
            is_null: true,
        };
        use S::*;
        #[derive(Debug, Clone, Copy)]
        pub enum S {
            Same,
            NotSame,
            Empty,
            /// A border point, which is not part of the board.
            Border,
        }
        assert!(!stone.is_empty());
        let lines = self
            .all_lines()
            .map(|(d, i)| {
                (
                    d,
                    std::iter::once([(Border, &NULL_POINT); 2])
                        .flatten()
                        .chain(i.map(|s| {
                            let s = self.get_xy(s.x, s.y).expect("should be populated");
                            if s.color.is_empty() {
                                (Empty, &s.point)
                            } else if s.color == stone {
                                (Same, &s.point)
                            } else {
                                (NotSame, &s.point)
                            }
                        }))
                        .chain(std::iter::once([(Border, &NULL_POINT); 2]).flatten())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<(Direction, Vec<_>)>>();
        let mut conditions = BTreeSet::new();
        let mut forbidden = BTreeSet::new();

        let mut fives = BTreeSet::new();

        tracing::debug!("checking fives");
        for (dir, stone_line) in &lines {
            for line in stone_line.windows(7) {
                // if let Some(only) = only_including {
                //     if !line.iter().any(|(_, p)| only.contains(p)) {
                //         continue;
                //     }
                // }
                match line {
                    // %XXXX_%
                    [(left, _), (Same, s0), (Same, s1), (Same, s2), (Same, s3), (Empty, s4), (right, _)] =>
                    {
                        if stone.is_black() && (matches!(right, Same) || matches!(left, Same)) {
                            continue;
                        }
                        let cond = RenjuCondition::Five {
                            direction: *dir,
                            stones: [**s0, **s1, **s2, **s3, **s4],
                            place: [**s4],
                        };
                        conditions.insert(cond);
                        fives.insert(s4);
                    }
                    // %_XXXX%
                    [(left, _), (Empty, s0), (Same, s1), (Same, s2), (Same, s3), (Same, s4), (right, _)] =>
                    {
                        if stone.is_black() && (matches!(left, Same) || matches!(right, Same)) {
                            continue;
                        }
                        let cond = RenjuCondition::Five {
                            direction: *dir,
                            stones: [**s0, **s1, **s2, **s3, **s4],
                            place: [**s0],
                        };
                        conditions.insert(cond);
                        fives.insert(s0);
                    }
                    _ => {}
                }
            }
        }

        // First check for overlines.
        tracing::debug!("checking overlines");
        if stone.is_black() {
            for (_, stone_line) in &lines {
                for line in stone_line.windows(6) {
                    // if let Some(only) = only_including {
                    //     if !line.iter().any(|(_, p)| only.contains(p)) {
                    //         continue;
                    //     }
                    // }
                    match line {
                        [(Empty, f), (Same, _), (Same, _), (Same, _), (Same, _), (Same, _)] => {
                            forbidden.insert(**f);
                        }
                        [(Same, _), (Empty, f), (Same, _), (Same, _), (Same, _), (Same, _)] => {
                            forbidden.insert(**f);
                        }
                        [(Same, _), (Same, _), (Empty, f), (Same, _), (Same, _), (Same, _)] => {
                            forbidden.insert(**f);
                        }
                        [(Same, _), (Same, _), (Same, _), (Empty, f), (Same, _), (Same, _)] => {
                            forbidden.insert(**f);
                        }
                        [(Same, _), (Same, _), (Same, _), (Same, _), (Empty, f), (Same, _)] => {
                            forbidden.insert(**f);
                        }
                        [(Same, _), (Same, _), (Same, _), (Same, _), (Same, _), (Empty, f)] => {
                            forbidden.insert(**f);
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut fours = BTreeMap::new();

        tracing::debug!("checking fours");
        for (dir, stone_line) in &lines {
            for line in stone_line.windows(7) {
                if let Some(only) = only_including {
                    if !line.iter().any(|(_, p)| only.contains(p)) {
                        continue;
                    }
                }
                match line {
                    // %._XXX%
                    // %_.XXX%
                    [(left, _), (Empty, s0), (Empty, s1), (Same, s2), (Same, s3), (Same, s4), (right, _)]
                        if matches!(right, Empty | NotSame | Border) =>
                    {
                        if !forbidden.contains(s1) {
                            let cond = match right {
                                Empty => RenjuCondition::StraightFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s1],
                                },
                                NotSame | Border => RenjuCondition::ClosedFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s1],
                                },
                                _ => unreachable!(),
                            };
                            fours.entry(s1).or_insert_with(BTreeSet::new).insert(cond);
                        }
                        if !forbidden.contains(s0) && matches!(left, Empty | NotSame | Border) {
                            let cond = RenjuCondition::BrokenFour {
                                direction: *dir,
                                stones: [**s0, **s1, **s2, **s3, **s4],
                                place: [**s0],
                            };
                            fours.entry(s0).or_insert_with(BTreeSet::new).insert(cond);
                        }
                    }
                    // %XXX_.%
                    // %XXX._%
                    [(left, _), (Same, s1), (Same, s2), (Same, s3), (Empty, s4), (Empty, s5), (right, _)]
                        if matches!(left, Empty | NotSame | Border) =>
                    {
                        if !forbidden.contains(s4) {
                            let cond = match left {
                                Empty => RenjuCondition::StraightFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s4],
                                },
                                NotSame | Border => RenjuCondition::ClosedFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s4],
                                },
                                _ => unreachable!(),
                            };
                            fours.entry(s4).or_insert_with(BTreeSet::new).insert(cond);
                        }
                        if !forbidden.contains(s5) && matches!(right, Empty | NotSame | Border) {
                            let cond = RenjuCondition::BrokenFour {
                                direction: *dir,
                                stones: [**s1, **s2, **s3, **s4, **s5],
                                place: [**s5],
                            };
                            fours.entry(s5).or_insert_with(BTreeSet::new).insert(cond);
                        }
                    }
                    // %.X_XX%
                    // %_X.XX%
                    [(left, _), (Empty, s0), (Same, s1), (Empty, s2), (Same, s3), (Same, s4), (right, _)]
                        if matches!(right, Empty | NotSame | Border) =>
                    {
                        if !forbidden.contains(s2) {
                            let cond = match right {
                                Empty => RenjuCondition::StraightFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s2],
                                },
                                _ => RenjuCondition::ClosedFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s2],
                                },
                            };
                            fours.entry(s2).or_insert_with(BTreeSet::new).insert(cond);
                        }
                        if !forbidden.contains(s0) && matches!(left, Empty | NotSame | Border) {
                            let cond = RenjuCondition::BrokenFour {
                                direction: *dir,
                                stones: [**s0, **s1, **s2, **s3, **s4],
                                place: [**s0],
                            };
                            fours.entry(s0).or_insert_with(BTreeSet::new).insert(cond);
                        }
                    }
                    // %XX_X.
                    // %XX.X_
                    [(left, _), (Same, s1), (Same, s2), (Empty, s3), (Same, s4), (Empty, s5), (right, _)]
                        if matches!(left, Empty | NotSame | Border) =>
                    {
                        if !forbidden.contains(s3) {
                            let cond = match left {
                                Empty => RenjuCondition::StraightFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s3],
                                },
                                _ => RenjuCondition::ClosedFour {
                                    direction: *dir,
                                    stones: [**s1, **s2, **s3, **s4],
                                    place: [**s3],
                                },
                            };
                            fours.entry(s3).or_insert_with(BTreeSet::new).insert(cond);
                        }
                        if !forbidden.contains(s5) && matches!(right, Empty | NotSame | Border) {
                            let cond = RenjuCondition::BrokenFour {
                                direction: *dir,
                                stones: [**s1, **s2, **s3, **s4, **s5],
                                place: [**s5],
                            };
                            fours.entry(s5).or_insert_with(BTreeSet::new).insert(cond);
                        }
                    }
                    _ => {}
                }
            }
        }

        for (k, v) in fours {
            if stone.is_black() && v.len() > 1 {
                forbidden.insert(**k);
            } else {
                conditions.extend(v);
            }
        }

        let mut threes = BTreeMap::new();

        // check for open threes, threes which can become straight fours. To do this, we need to check a huge range, 8 stones to be exact.
        tracing::debug!("checking threes");
        for (dir, stone_line) in &lines {
            for line in stone_line.windows(9) {
                if let Some(only) = only_including {
                    if !line.iter().any(|(_, p)| only.contains(p)) {
                        continue;
                    }
                }
                match line {
                    // %.__XX.%
                    [(left, _), (Empty, _s1), (Empty, s2), (Empty, s3), (Same, s4), (Same, s5), (Empty, _s6), (right, _), (eh_case, _)] =>
                    {
                        match (left, right) {
                            (_, Same) => {
                                continue;
                            }
                            // X..xXX.%
                            (Same, Border | NotSame | Empty) => {
                                // there is a very special case here, if x.._xx..x, then it's not a three, since that three does not given a open four
                                if stone.is_black() && matches!(eh_case, Same) {
                                    continue;
                                }
                            }
                            (Border | NotSame | Empty, Border | NotSame | Empty) => {
                                if !forbidden.contains(s2)
                                    && !fives.contains(s2)
                                    && !fives.contains(s3)
                                {
                                    let cond = RenjuCondition::BrokenThree {
                                        direction: *dir,
                                        stones: [**s2, **s3, **s4, **s5],
                                        place: [**s2],
                                    };
                                    threes
                                        .entry(s2)
                                        .or_insert_with(BTreeSet::new)
                                        .insert((cond, *s3));
                                }
                            }
                        }
                        if !forbidden.contains(s3) && !fives.contains(s3) && !fives.contains(s2) {
                            let cond = RenjuCondition::UnbrokenThree {
                                direction: *dir,
                                stones: [**s3, **s4, **s5],
                                place: [**s3],
                            };
                            threes
                                .entry(s3)
                                .or_insert_with(BTreeSet::new)
                                .insert((cond, *s2));
                        }
                    }
                    // %.XX__.%
                    [(eh_case, _), (left, _), (Empty, _s1), (Same, s2), (Same, s3), (Empty, s4), (Empty, s5), (Empty, _s6), (right, _)] =>
                    {
                        match (left, right) {
                            (Same, _) => {
                                continue;
                            }
                            // X..xXX.%
                            (Border | NotSame | Empty, Same) => {
                                // there is a very special case here, if x..xx_..x, then it's not a three, since that three does not given a open four
                                if stone.is_black() && matches!(eh_case, Same) {
                                    continue;
                                }
                            }
                            (Border | NotSame | Empty, Border | NotSame | Empty) => {
                                if !forbidden.contains(s5)
                                    && !fives.contains(s5)
                                    && !fives.contains(s4)
                                {
                                    let cond = RenjuCondition::BrokenThree {
                                        direction: *dir,
                                        stones: [**s2, **s3, **s4, **s5],
                                        place: [**s5],
                                    };
                                    threes
                                        .entry(s5)
                                        .or_insert_with(BTreeSet::new)
                                        .insert((cond, *s4));
                                }
                            }
                        }
                        if !forbidden.contains(s4) && !fives.contains(s4) && !fives.contains(s5) {
                            let cond = RenjuCondition::UnbrokenThree {
                                direction: *dir,
                                stones: [**s2, **s3, **s4],
                                place: [**s4],
                            };
                            threes
                                .entry(s4)
                                .or_insert_with(BTreeSet::new)
                                .insert((cond, *s5));
                        }
                    }

                    // %._X_X.%
                    [(left, _s0), (Empty, _s1), (Empty, s2), (Same, s3), (Empty, s4), (Same, s5), (Empty, _s6), (right, _s7), ..] =>
                    {
                        match (left, right) {
                            (_, Same) => {
                                continue;
                            }
                            (Same, Border | NotSame | Empty) => {}
                            (Border | NotSame | Empty, Border | NotSame | Empty) => {
                                if !forbidden.contains(s2)
                                    && !fives.contains(s2)
                                    && !fives.contains(s4)
                                {
                                    let cond = RenjuCondition::BrokenThree {
                                        direction: *dir,
                                        stones: [**s2, **s3, **s4, **s5],
                                        place: [**s2],
                                    };
                                    threes
                                        .entry(s2)
                                        .or_insert_with(BTreeSet::new)
                                        .insert((cond, *s4));
                                }
                            }
                        }
                        if !forbidden.contains(s4) && !fives.contains(s4) && !fives.contains(s2) {
                            let cond = RenjuCondition::UnbrokenThree {
                                direction: *dir,
                                stones: [**s3, **s4, **s5],
                                place: [**s4],
                            };
                            threes
                                .entry(s4)
                                .or_insert_with(BTreeSet::new)
                                .insert((cond, *s2));
                        }
                    }

                    // %.X_X_.%
                    [(left, _s0), (Empty, _s1), (Same, s2), (Empty, s3), (Same, s4), (Empty, s5), (Empty, _s6), (right, _s7), ..] =>
                    {
                        match (left, right) {
                            (Same, _) => {
                                continue;
                            }
                            (Border | NotSame | Empty, Same) => {}
                            (Border | NotSame | Empty, Border | NotSame | Empty) => {
                                if !forbidden.contains(s5)
                                    && !fives.contains(s5)
                                    && !fives.contains(s4)
                                {
                                    let cond = RenjuCondition::BrokenThree {
                                        direction: *dir,
                                        stones: [**s2, **s3, **s4, **s5],
                                        place: [**s5],
                                    };
                                    threes
                                        .entry(s5)
                                        .or_insert_with(BTreeSet::new)
                                        .insert((cond, *s4));
                                }
                            }
                        }
                        if !forbidden.contains(s3) && !fives.contains(s3) && !fives.contains(s5) {
                            let cond = RenjuCondition::UnbrokenThree {
                                direction: *dir,
                                stones: [**s2, **s3, **s4],
                                place: [**s3],
                            };
                            threes
                                .entry(s3)
                                .or_insert_with(BTreeSet::new)
                                .insert((cond, *s5));
                        }
                    }
                    // %.X__X.%
                    [(Border | NotSame | Empty, _s1), (Empty, _s2), (Same, s3), (Empty, s4), (Empty, s5), (Same, s6), (Empty, _s7), (Border | NotSame | Empty, _s8), ..] =>
                    {
                        if !forbidden.contains(s4) && !fives.contains(s4) && !fives.contains(s5) {
                            let cond = RenjuCondition::BrokenThree {
                                direction: *dir,
                                stones: [**s3, **s4, **s5, **s6],
                                place: [**s4],
                            };
                            threes
                                .entry(s4)
                                .or_insert_with(BTreeSet::new)
                                .insert((cond, *s5));
                        }
                        if !forbidden.contains(s5) && !fives.contains(s5) && !fives.contains(s4) {
                            let cond = RenjuCondition::BrokenThree {
                                direction: *dir,
                                stones: [**s3, **s4, **s5, **s6],
                                place: [**s5],
                            };
                            threes
                                .entry(s5)
                                .or_insert_with(BTreeSet::new)
                                .insert((cond, *s4));
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut found_forbidden_threes = BTreeSet::new();
        for (k, v) in &threes {
            let span = tracing::debug_span!("3FCheck", ?k);
            let _enter = span.enter();
            // we need to check the unique conditions, based on the row, not the point which could be a four.
            let three_row: BTreeSet<&RenjuCondition> = v.iter().map(|(c, _)| c).collect();
            if stone.is_black() && three_row.len() > 1 {
                tracing::debug!(?k, ?v, "is k forbidden?");

                // 9.3 A black double-three is allowed if at least one of the following conditions  a) or b) is/are fulfilled:
                // a) Not more than one of the three's can be made to a straight four when adding another stone in just any intersection,
                //    without at the same time an overline or double-four is attained in this intersection.
                //    To find out which double-three's, which are allowed, you must make the move, which causes the double-three,
                //    in your mind, and then continue trying to make straight fours, which are allowed, in your mind.
                // b) Not more than one of the three's can be made to a straight four when adding another stone in just any intersection,
                //    without at the same time at least two three's meet in this intersection and make a forbidden double-three.
                //    To find out if this last double-three is forbidden or not, you must at first examine if
                //    the double-three is allowed according to a) above, and then in your mind continue trying to make straight fours of the three's in your mind.
                //    If, when making a straight four in your mind, another double-three would be attained also these double-three's must be examined
                //    in the same way as it is described in this point 9.3, etc.

                // First check for overlines and double-four, case a).
                let mut allowed_fours = 0;
                for (c, four_point) in v.iter() {
                    if !forbidden.contains(four_point) {
                        tracing::debug!(
                            ?c,
                            ?four_point,
                            "found a four_point that does not cause a overline or double-four"
                        );
                        allowed_fours += 1;
                    }
                }
                // a) is not fulfilled, we need to check a)
                // that is, if there's more than one way to do a straight four
                if allowed_fours > 1 {
                    tracing::debug!(
                        ?k,
                        ?allowed_fours,
                        "found {allowed_fours:?} fours that are allowed, need to check if there's more than one three that is allowed threes"
                    );
                    let mut allowed_threes = v.len();
                    // Check for double-three, case b).
                    let mut new_board = self.clone();
                    new_board.set_point(***k, stone);
                    tracing::debug!("new board\n: {new_board}");
                    tracing::debug!(stones_added = ?[&k,], "adding stones to board to check for double-three.");
                    for (_c, four_point) in v.iter() {
                        let span = tracing::debug_span!("four_point check", ?four_point,);
                        let _enter = span.enter();
                        // FIXME: Instead of cloning, we could mutate the board, marking the added stones as special somehow, this would minimize memory allocation, but, wouldn't allow parallelization.

                        // TODO
                        tracing::debug!("checking if the four_point is forbidden double-three");
                        let new_conditions =
                            new_board.renju_conditions(stone, Some(&[***k, **four_point]));

                        tracing::debug!("checked if the four_point is forbidden double-three");
                        // Now, check condition if more than one allowed straight four can be  made
                        if new_conditions
                            .conditions
                            .iter()
                            .filter(|c| {
                                matches!(c, RenjuCondition::StraightFour { .. })
                                    && c.stones().contains(**k)
                            })
                            .filter(|c| !new_conditions.forbidden.contains(c.place()))
                            .count()
                            > 1
                        {
                        } else {
                            tracing::debug!(?new_conditions, "got new conditions");
                            if new_conditions.forbidden.contains(four_point) {
                                tracing::debug!(?four_point, "found a forbidden double-three");
                                allowed_threes -= 1;
                            } else {
                                tracing::debug!(?four_point, "found a allowed double-three");
                            }
                        }
                    }
                    if allowed_threes > 1 {
                        tracing::debug!(
                            ?k,
                            ?allowed_threes,
                            "b) is not fulfilled, the double-three on {k:?} is forbidden."
                        );
                        found_forbidden_threes.insert(**k);
                    } else {
                        tracing::debug!(?k, "b) is fulfilled, the double-three is allowed.");
                    }
                } else {
                    tracing::debug!(?k, "a) is fulfilled, the double-three is allowed.");
                }
            } else {
                conditions.extend(v.iter().map(|(c, _)| c.clone()));
            }
        }
        forbidden.extend(found_forbidden_threes.clone());

        if stone.is_white() {
            assert!(forbidden.is_empty());
        }

        if stone.is_white() {
            assert!(forbidden.is_empty());
        }

        RenjuConditions {
            conditions,
            forbidden,
            threes: threes
                .into_iter()
                .flat_map(|(_k, v)| v.into_iter().map(|(c, p)| (c, *p)))
                .collect(),
        }
    }

    fn all_lines(&self) -> impl Iterator<Item = (Direction, impl Iterator<Item = Point>)> + '_ {
        let size = self.size();
        std::iter::empty()
            .chain(
                // Horizontal
                (0..size).map(move |y| {
                    (
                        Direction::Horizontal,
                        self.get_line(Direction::Horizontal, &Point::new(0, y)).1,
                    )
                }),
            )
            .chain(
                // vertical
                (0..size).map(move |x| {
                    (
                        Direction::Vertical,
                        self.get_line(Direction::Vertical, &Point::new(x, 0)).1,
                    )
                }),
            )
            .chain(
                // Diagonal /

                // walk across in \
                (0..size).flat_map(move |i| {
                    [
                        (
                            Direction::Diagonal { bottom: true },
                            self.get_line(Direction::Diagonal { bottom: true }, &Point::new(0, i))
                                .1,
                        ),
                        (
                            Direction::Diagonal { bottom: true },
                            self.get_line(
                                Direction::Diagonal { bottom: true },
                                &Point::new(size, i),
                            )
                            .1,
                        ),
                    ]
                }),
            )
            .chain(
                // Diagonal \
                (0..size).flat_map(move |i| {
                    [
                        (
                            Direction::Diagonal { bottom: false },
                            self.get_line(
                                Direction::Diagonal { bottom: false },
                                &Point::new(0, size - 1 - i),
                            )
                            .1,
                        ),
                        (
                            Direction::Diagonal { bottom: false },
                            self.get_line(
                                Direction::Diagonal { bottom: false },
                                &Point::new(size, size - 1 - i),
                            )
                            .1,
                        ),
                    ]
                }),
            )
    }

    /// Get the positions of a line on a board. First `usize` is the index of the point itself in the iterator.
    fn get_line(
        &self,
        direction: Direction,
        point: &Point,
    ) -> (usize, impl Iterator<Item = Point>) {
        // idx is the index of the point itself in the iterator
        let idx;
        // The first point
        let start = match direction {
            Direction::Horizontal => {
                idx = point.x;
                // on horizontal -, we need the leftmost point on this row
                Point::new(0, point.y)
            }
            Direction::Vertical => {
                idx = point.y;
                // on vertical |, we need the topmost point on this column
                Point::new(point.x, 0)
            }
            Direction::Diagonal { bottom: true } => {
                // on diagonal /, we need the diagonal bottom leftmost point
                let steps = std::cmp::min(point.x, self.size() - 1 - point.y);
                idx = steps;
                Point::new(point.x - steps, point.y + steps)
            }
            Direction::Diagonal { bottom: false } => {
                // on diagonal \, we need the diagonal top leftmost point
                let steps = std::cmp::min(point.x, point.y);
                idx = steps;
                Point::new(point.x - steps, point.y - steps)
            }
        };
        let mut count = 0;
        (
            idx as usize,
            std::iter::from_fn(move || {
                let next = match direction {
                    Direction::Horizontal => Point::new(start.x + count, start.y),
                    Direction::Vertical => Point::new(start.x, start.y + count),
                    Direction::Diagonal { bottom: true } => {
                        Point::new(start.x + count, start.y.checked_sub(count)?)
                    }
                    Direction::Diagonal { bottom: false } => {
                        Point::new(start.x + count, start.y + count)
                    }
                };
                count += 1;
                if next.is_valid() {
                    Some(next)
                } else {
                    None
                }
            })
            .fuse(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        board::{BoardArr, BoardMarker, Point, Stone},
        p,
    };
    use test_log::test;

    #[test]
    fn test_condition() {
        let mut board = BoardArr::new(15);
        for pos in p![[H, 8], [G, 8], [G, 9], [H, 10]] {
            board.set_point(pos, Stone::Black);
        }
        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        assert_eq!(conditions.forbidden, p![[F, 8]].iter().copied().collect());

        tracing::info!("forbidden {:?}", conditions.forbidden);
        let mut board = BoardArr::new(15);

        for pos in p![
            [C, 13],
            [E, 13],
            [K, 13],
            [N, 13],
            [D, 12],
            [M, 12],
            [M, 11],
            [D, 10],
            [I, 10],
            [G, 8],
            [H, 8],
            [H, 7],
            [C, 5],
            [N, 5],
            [D, 4],
            [F, 4],
            [M, 4],
            [D, 2],
            [I, 2],
            [J, 2]
        ] {
            board.set_point(pos, Stone::Black);
        }

        board.set_point(p![B, 13], Stone::White);

        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        assert_eq!(
            conditions.forbidden,
            p![[H, 9], [M, 13], [E, 3], [K, 2]]
                .iter()
                .copied()
                .collect()
        );
        tracing::info!("forbidden {:?}", conditions.forbidden);

        let mut board = BoardArr::new(15);
        for pos in p![
            [E, 13],
            [C, 12],
            [E, 12],
            [B, 11],
            [E, 11],
            [A, 10],
            [E, 8],
            [F, 8],
            [H, 8],
            [I, 8],
            [J, 8],
            [B, 4],
            [K, 4],
            [L, 4],
            [B, 3],
            [D, 3],
            [J, 3],
            [L, 3],
            [D, 2]
        ] {
            board.set_point(pos, Stone::Black);
        }

        board.set_point(p![E, 10], Stone::White);

        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        assert_eq!(
            conditions.forbidden,
            p![[E, 14], [G, 8], [L, 5]].iter().copied().collect()
        );

        let mut board = BoardArr::new(15);
        for pos in p![
            [O, 14],
            [E, 13],
            [A, 12],
            [E, 12],
            [F, 12],
            [I, 12],
            [C, 11],
            [L, 11],
            [N, 11],
            [K, 10],
            [N, 10],
            [J, 9],
            [N, 9],
            [C, 5],
            [C, 4],
            [F, 4],
            [E, 3]
        ] {
            board.set_point(pos, Stone::Black);
        }

        for pos in p![[I, 8], [G, 1], [N, 8]] {
            board.set_point(pos, Stone::White);
        }

        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        // N13 and D12 are not forbidden
        assert_eq!(
            conditions.forbidden,
            p![[D, 4], [M, 10]].iter().copied().collect()
        );
    }

    #[test]
    fn tricky_forbidden() {
        let mut board = BoardArr::new(15);

        for pos in p![
            [K, 8],
            [J, 8],
            [I, 7],
            [I, 6],
            [H, 7],
            [H, 6],
            [N, 8],
            [F, 8]
        ] {
            board.set_point(pos, Stone::Black);
        }
        for pos in p![[J, 7], [G, 7]] {
            board.set_point(pos, Stone::White);
        }
        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        assert_eq!(conditions.forbidden, BTreeSet::new(),)
    }

    // This test is ignored since it's a very tricky case, and I don't know how to solve it yet
    // See https://github.com/dhbloo/rapfi/blob/b9e89301f476fe8acc3ef876f73a27664498c6de/Rapfi/game/board.cpp#L434
    #[test]
    #[ignore]
    fn even_trickier_forbidden() {
        let mut board = BoardArr::new(15);

        let mut stone = Stone::Black;
        for pos in p![
            [D, 14],
            [E, 14],
            [C, 13],
            [C, 12],
            [D, 12],
            [A, 9],
            [C, 11],
            [E, 11],
            [F, 12],
            [G, 12],
            [F, 13],
            [N, 13],
            [H, 12],
            [N, 11],
            [I, 11],
            [E, 12],
        ] {
            board.set_point(pos, stone);
            stone = stone.opposite();
        }

        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        // According to https://www.aiexp.info/pages/wiki-three-def.html, [D, 13] is not forbidden.
        // Some might say that it should be forbidden, but it isn't due to 9.3 in the [RIF Rules](https://www.renju.net/rifrules/) and the definition of a three excluding threes whose four point create a five.
        // A black double-three is allowed if at least one of the following conditions  a)   or  b) is/are fulfilled:
        // a) Not more than one of the three's can be made to a straight four when adding another stone in just any intersection, without at the same time an overline or double-four is attained in this intersection.
        // b) Not more than one of the three's can be made to a straight four when adding another stone in just any intersection, without at the same time at least two three's meet in this intersection and make a forbidden double-three.
        // So, D13 is not forbidden since F14 as a broken three is not a three, it's a false three, since it would create a five.
        assert_eq!(
            conditions.forbidden,
            p![[E, 13], [F, 14]].iter().copied().collect(),
        )
    }

    #[test]
    fn rif_definition_of_three() {
        // A row with three stones to which you, without at the same time a five in a row is made, can add one more stone to attain a straight four.
        let mut board = BoardArr::new(15);

        for p in p![
            [F, 8],
            [F, 9],
            [F, 10],
            [F, 11],
            [G, 7],
            [H, 7],
            [G, 13],
            [H, 13],
            [I, 13],
            [J, 13],
            [L, 12],
            [M, 11]
        ] {
            board.set_point(p, Stone::Black);
        }
        tracing::info!("board \n{}", board);

        let conditions = board.renju_conditions(Stone::Black, Some(&p![[F, 7], [K, 13]]));
        let f7: Vec<_> = conditions
            .threes
            .iter()
            .filter(|c| c.0.stones().contains(&p![F, 7]))
            .collect();
        tracing::info!("{f7:?}");
        let k13: Vec<_> = conditions
            .threes
            .iter()
            .filter(|c| c.0.stones().contains(&p![K, 13]))
            .collect();
        tracing::info!("{k13:?}");
        tracing::info!("{conditions:?}");
    }

    #[test]
    #[rustfmt::skip]
    fn line() {
        let board = BoardArr::new(15);

        let p = p![H, 8];

        // Horizontal
        let line = board.get_line(Direction::Horizontal, &p).1.collect::<Vec<_>>();
        let actual = p![[A,  8], [B,  8], [C,  8], [D,  8], [E,  8], [F,  8], [G,  8], [H,  8], [I,  8], [J,  8], [K,  8], [L,  8], [M,  8], [N,  8], [O,  8]];
        tracing::info!("{actual:?}");
        assert_eq!(line, actual);
        for p in &line {
            let line = board.get_line(Direction::Horizontal, p).1.collect::<Vec<_>>();
            assert_eq!(line, actual);
        }

        // Vertical
        let line = board.get_line(Direction::Vertical, &p).1.collect::<Vec<_>>();
        let actual = p![[H, 15], [H, 14], [H, 13], [H, 12], [H, 11], [H, 10], [H,  9], [H,  8], [H,  7], [H,  6], [H,  5], [H,  4], [H,  3], [H,  2], [H,  1]];
        tracing::info!("{actual:?}");
        assert_eq!(line, actual);
        for p in &line {
            let line = board.get_line(Direction::Vertical, p).1.collect::<Vec<_>>();
            assert_eq!(line, actual);
        }

        // Diagonal /
        let line = board.get_line(Direction::Diagonal { bottom: true }, &p).1.collect::<Vec<_>>();
        let actual = p![[A,  1], [B,  2], [C,  3], [D,  4], [E,  5], [F,  6], [G,  7], [H,  8], [I,  9], [J, 10], [K, 11], [L, 12], [M, 13], [N, 14], [O, 15]];
        tracing::info!("{actual:?}");
        assert_eq!(line, actual);
        for p in &line {
            let line = board.get_line(Direction::Diagonal { bottom: true }, p).1.collect::<Vec<_>>();
            assert_eq!(line, actual);
        }

        // Diagonal \
        let line = board.get_line(Direction::Diagonal { bottom: false }, &p).1.collect::<Vec<_>>();
        let actual = p![[A, 15], [B, 14], [C, 13], [D, 12], [E, 11], [F, 10], [G,  9], [H,  8], [I,  7], [J,  6], [K,  5], [L,  4], [M,  3], [N,  2], [O,  1]];
        tracing::info!("{actual:?}");
        assert_eq!(line, actual);
        for p in &line {
            let line = board.get_line(Direction::Diagonal { bottom: false }, p).1.collect::<Vec<_>>();
            assert_eq!(line, actual);
        }

        // other diagonals, starting on other points
        let p = p![G, 8];

        // Diagonal /
        let line = board.get_line(Direction::Diagonal { bottom: true }, &p).1.collect::<Vec<_>>();
        let actual = p![[A,  2], [B,  3], [C,  4], [D,  5], [E,  6], [F,  7], [G,  8], [H,  9], [I, 10], [J, 11], [K, 12], [L, 13], [M, 14], [N, 15]];
        tracing::info!("{actual:?}");
        assert_eq!(line, actual);
        for p in &line {
            let line = board.get_line(Direction::Diagonal { bottom: true }, p).1.collect::<Vec<_>>();
            assert_eq!(line, actual);
        }

        // Diagonal \
        let line = board.get_line(Direction::Diagonal { bottom: false }, &p).1.collect::<Vec<_>>();
        let actual = p![[A, 14], [B, 13], [C, 12], [D, 11], [E, 10], [F,  9], [G,  8], [H,  7], [I,  6], [J,  5], [K,  4], [L,  3], [M,  2], [N,  1]];
        tracing::info!("{actual:?}");
        assert_eq!(line, actual);
        for p in &line {
            let line = board.get_line(Direction::Diagonal { bottom: false }, p).1.collect::<Vec<_>>();
            assert_eq!(line, actual);
        }

        // special diagonals
        let p = p![A, 15];
        let line = board.get_line(Direction::Diagonal { bottom: true }, &p).1.collect::<Vec<_>>();
        let actual = p![[A, 15]];
        assert_eq!(line, actual);

        let p = p![A, 14];
        let line = board.get_line(Direction::Diagonal { bottom: true }, &p).1.collect::<Vec<_>>();
        let actual = p![[A, 14], [B, 15]];
        assert_eq!(line, actual);

        let p = p![A, 1];
        let line = board.get_line(Direction::Diagonal { bottom: false }, &p).1.collect::<Vec<_>>();
        let actual = p![[A, 1]];
        assert_eq!(line, actual);

        let p = p![A, 2];
        let line = board.get_line(Direction::Diagonal { bottom: false }, &p).1.collect::<Vec<_>>();
        let actual = p![[A, 2], [B, 1]];
        assert_eq!(line, actual);
    }

    #[test]
    fn all_lines_is_all_lines_and_not_twice() {
        let board = BoardArr::new(15);
        let mut all_lines = BTreeMap::new();

        for (dir, iter) in board.all_lines() {
            all_lines.entry(dir).or_insert(vec![]).extend(iter);
        }
        for (dir, points) in all_lines {
            let mut board = (*board).clone();
            let mut found = BTreeMap::new();
            for p in points {
                board.retain(|i| i.point != p);
                *found.entry(p).or_insert(0) += 1;
            }
            let mut disp_board = BoardArr::new(15);
            for p in &board {
                disp_board.set_point(p.point, Stone::Black);
            }
            assert!(
                board.is_empty(),
                "{:?} was not empty, left: \n{}",
                dir,
                disp_board
            );

            for (k, v) in found {
                assert_eq!(v, 1, "{:?} was found multiple times", k);
            }
        }
    }

    #[test]
    fn check_if_illegal_move() {
        let mut board = BoardArr::new(15);
        for pos in p![[H, 8], [G, 8], [G, 9], [H, 10]] {
            board.set_point(pos, Stone::Black);
        }

        let conditions = board.renju_conditions(Stone::Black, None);
        for forbidden in &conditions.forbidden {
            board
                .get_point_mut(*forbidden)
                .unwrap()
                .set_oneline_comment("*".to_owned());
        }
        tracing::debug!("board \n{}", board);
        assert_eq!(conditions.forbidden, p![[F, 8]].iter().copied().collect());
    }

    #[test]
    fn is_horizontal_five_in_a_row() {
        let mut board = BoardArr::new(15);
        let y = 7u32;
        let p1 = BoardMarker::new(Point::new(4, y), Stone::Black);
        for x in 0..4 {
            board.set_point(Point::new(x, y), Stone::Black);
        }

        let p2 = BoardMarker::new(Point::new(8, y + 2), Stone::White);
        for x in (7..12).filter(|x| *x != 8) {
            board.set_point(Point::new(x, y + 2), Stone::White);
        }
        tracing::info!("\n{}\nChecks,{:?} and {:?}", board, &p1, p2);
        //assert!(is_five_dir(&board, &p1, Direction::Horizontal).unwrap());
        //assert!(is_five_dir(&board, &p2, Direction::Horizontal).unwrap());
        //assert_eq!(line(&board, &p1), Ok(Direction::Horizontal));
        // assert_eq!(is_line(&board, &p2).unwrap(), Direction::Horizontal);
    }

    #[test]
    fn is_vertical_five_in_a_row() {
        let mut board = BoardArr::new(15);
        let x = 7u32;
        let p1 = BoardMarker::new(Point::new(x, 4), Stone::Black);
        for y in 0..4 {
            board.set_point(Point::new(x, y), Stone::Black);
        }

        let p2 = BoardMarker::new(Point::new(x + 2, 8), Stone::White);
        for y in (7..12).filter(|y| *y != 8) {
            board.set_point(Point::new(x + 2, y), Stone::White);
        }
        tracing::info!("\n{}\nChecks; {:?} and {:?}", board, &p1, p2);

        //assert!(is_five_dir(&board, &p1, Direction::Vertical).unwrap());
        //assert!(is_five_dir(&board, &p2, Direction::Vertical).unwrap());
        //assert_eq!(is_line(&board, &p1), Ok(Direction::Vertical));
        //assert_eq!(is_line(&board, &p2), Ok(Direction::Vertical));
    }

    #[test]
    fn is_diagonal_five_in_a_row() {
        let mut board = BoardArr::new(15);
        // A diagonal is '\'
        for pos in &[2u32 + 7 * 15, 3u32 + 8 * 15, 4u32 + 9 * 15, 5u32 + 10 * 15] {
            board.set_point(Point::from_1d(*pos, 15), Stone::Black);
        }
        #[allow(clippy::identity_op)]
        for pos in &[
            9u32, /*+ 0 * 15*/
            10u32 + 1 * 15,
            11u32 + 2 * 15,
            13u32 + 4 * 15,
        ] {
            board.set_point(Point::from_1d(*pos, 15), Stone::White);
        }

        let p1 = BoardMarker::new(Point::from_1d(11 * 15 + 6, 15), Stone::Black);
        let p2 = BoardMarker::new(Point::from_1d(12 + 3 * 15, 15), Stone::White);

        tracing::info!("\n{}\nChecks; {:?} and {:?}", board, &p1, p2);

        //assert!(is_five_dir(&board, &p1, Direction::Diagonal).unwrap());
        //assert!(is_five_dir(&board, &p2, Direction::Diagonal).unwrap());
        //assert_eq!(is_line(&board, &p1), Ok(Direction::Diagonal));
        //assert_eq!(is_line(&board, &p2), Ok(Direction::Diagonal));
    }
    #[test]
    fn is_anti_diagonal_five_in_a_row() {
        let mut board = BoardArr::new(15);
        for pos in &[6u32 + 6 * 15, 5u32 + 7 * 15, 4u32 + 8 * 15, 3u32 + 9 * 15] {
            board.set_point(Point::from_1d(*pos, 15), Stone::Black);
        }

        let p1 = BoardMarker::new(Point::from_1d(2u32 + 10 * 15, 15), Stone::Black);

        tracing::info!("\n{}\nChecks; {:?}", board, &p1);
        //assert!(is_five_dir(&board, &p1, Direction::AntiDiagonal).unwrap());
        //assert_eq!(is_line(&board, &p1), Ok(Direction::AntiDiagonal));
    }
}
