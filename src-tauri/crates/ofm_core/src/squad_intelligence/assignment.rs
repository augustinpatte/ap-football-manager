//! The optimal assignment of players to slots, shared by the squad model and the lineup picker so
//! that the side a signing is valued by is the side the manager fields.

/// The assignment algorithm and the buffers it works in.
#[derive(Default)]
pub(crate) struct Solver {
    row_potential: Vec<i64>,
    column_potential: Vec<i64>,
    matched_row: Vec<usize>,
    way: Vec<usize>,
    minimum: Vec<i64>,
    seen: Vec<bool>,
    /// The column of each row after [`Solver::solve`].
    pub(crate) result: Vec<usize>,
}

impl Solver {
    /// Finds, for each of `rows` rows (`rows <= columns`), the column that minimises the total of
    /// `cost[row * columns + column]`, each column used once, and leaves it in `self.result`. The
    /// classic potentials-and-augmenting-path form: `O(rows^2 * columns)`.
    pub(crate) fn solve(&mut self, cost: &[i64], rows: usize, columns: usize) {
        const INF: i64 = i64::MAX / 4;
        self.row_potential.clear();
        self.row_potential.resize(rows + 1, 0);
        self.column_potential.clear();
        self.column_potential.resize(columns + 1, 0);
        self.matched_row.clear();
        self.matched_row.resize(columns + 1, 0);
        self.way.clear();
        self.way.resize(columns + 1, 0);
        for row in 1..=rows {
            self.matched_row[0] = row;
            let mut column = 0;
            self.minimum.clear();
            self.minimum.resize(columns + 1, INF);
            self.seen.clear();
            self.seen.resize(columns + 1, false);
            loop {
                self.seen[column] = true;
                let current = self.matched_row[column];
                let mut delta = INF;
                let mut next = 0;
                for candidate in 1..=columns {
                    if self.seen[candidate] {
                        continue;
                    }
                    let reduced = cost[(current - 1) * columns + candidate - 1]
                        - self.row_potential[current]
                        - self.column_potential[candidate];
                    if reduced < self.minimum[candidate] {
                        self.minimum[candidate] = reduced;
                        self.way[candidate] = column;
                    }
                    if self.minimum[candidate] < delta {
                        delta = self.minimum[candidate];
                        next = candidate;
                    }
                }
                for candidate in 0..=columns {
                    if self.seen[candidate] {
                        self.row_potential[self.matched_row[candidate]] += delta;
                        self.column_potential[candidate] -= delta;
                    } else {
                        self.minimum[candidate] -= delta;
                    }
                }
                column = next;
                if self.matched_row[column] == 0 {
                    break;
                }
            }
            loop {
                let previous = self.way[column];
                self.matched_row[column] = self.matched_row[previous];
                column = previous;
                if column == 0 {
                    break;
                }
            }
        }
        self.result.clear();
        self.result.resize(rows, 0);
        for column in 1..=columns {
            if self.matched_row[column] != 0 {
                self.result[self.matched_row[column] - 1] = column - 1;
            }
        }
    }
}
