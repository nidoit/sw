//! Maya calendar arithmetic (pure Rust, no browser dependencies → unit-testable).
//!
//! Uses the GMT (Goodman–Martinez–Thompson) correlation constant 584283:
//! Long Count 0.0.0.0.0 (4 Ajaw 8 Kumk'u) = Julian Day Number 584283.

pub const GMT_CORRELATION: i64 = 584_283;

pub const TZOLKIN_NAMES: [&str; 20] = [
    "Imix", "Ik'", "Ak'b'al", "K'an", "Chikchan", "Kimi", "Manik'", "Lamat", "Muluk", "Ok",
    "Chuwen", "Eb'", "B'en", "Ix", "Men", "Kib'", "Kab'an", "Etz'nab'", "Kawak", "Ajaw",
];

pub const HAAB_NAMES: [&str; 19] = [
    "Pop", "Wo'", "Sip", "Sotz'", "Sek", "Xul", "Yaxk'in", "Mol", "Ch'en", "Yax", "Sak'", "Keh",
    "Mak", "K'ank'in", "Muwan", "Pax", "K'ayab", "Kumk'u", "Wayeb'",
];

/// Julian Day Number of a proleptic Gregorian calendar date (noon-based integer JDN).
pub fn jdn_from_gregorian(year: i64, month: i64, day: i64) -> i64 {
    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    day + (153 * m + 2) / 5 + 365 * y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)
        - 32045
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MayaDate {
    /// Days elapsed since Long Count 0.0.0.0.0
    pub day_count: i64,
    /// b'ak'tun, k'atun, tun, winal, k'in
    pub long_count: [i64; 5],
    /// Tzolk'in number 1..=13
    pub tzolkin_number: u32,
    /// Tzolk'in day-name index 0..20 (0 = Imix, 19 = Ajaw)
    pub tzolkin_name: u32,
    /// Haab' day-of-month 0..20 (0..5 in Wayeb')
    pub haab_day: u32,
    /// Haab' month index 0..19 (18 = Wayeb')
    pub haab_month: u32,
}

impl MayaDate {
    pub fn from_gregorian(year: i64, month: i64, day: i64) -> Self {
        Self::from_day_count(jdn_from_gregorian(year, month, day) - GMT_CORRELATION)
    }

    pub fn from_day_count(d: i64) -> Self {
        let long_count = [
            d.div_euclid(144_000),
            d.rem_euclid(144_000) / 7_200,
            d.rem_euclid(7_200) / 360,
            d.rem_euclid(360) / 20,
            d.rem_euclid(20),
        ];
        let tzolkin_number = ((d + 3).rem_euclid(13) + 1) as u32;
        let tzolkin_name = (d + 19).rem_euclid(20) as u32;
        let haab_pos = (d + 348).rem_euclid(365);
        MayaDate {
            day_count: d,
            long_count,
            tzolkin_number,
            tzolkin_name,
            haab_day: (haab_pos % 20) as u32,
            haab_month: (haab_pos / 20) as u32,
        }
    }

    pub fn long_count_string(&self) -> String {
        let l = self.long_count;
        format!("{}.{}.{}.{}.{}", l[0], l[1], l[2], l[3], l[4])
    }

    pub fn tzolkin_string(&self) -> String {
        format!("{} {}", self.tzolkin_number, TZOLKIN_NAMES[self.tzolkin_name as usize])
    }

    pub fn haab_string(&self) -> String {
        format!("{} {}", self.haab_day, HAAB_NAMES[self.haab_month as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_date() {
        let m = MayaDate::from_day_count(0);
        assert_eq!(m.tzolkin_string(), "4 Ajaw");
        assert_eq!(m.haab_string(), "8 Kumk'u");
    }

    #[test]
    fn end_of_bakun_13_2012() {
        // 21 Dec 2012 = 13.0.0.0.0, 4 Ajaw 3 K'ank'in
        let m = MayaDate::from_gregorian(2012, 12, 21);
        assert_eq!(m.long_count, [13, 0, 0, 0, 0]);
        assert_eq!(m.tzolkin_string(), "4 Ajaw");
        assert_eq!(m.haab_string(), "3 K'ank'in");
    }

    #[test]
    fn jdn_known_values() {
        assert_eq!(jdn_from_gregorian(2000, 1, 1), 2_451_545);
        assert_eq!(jdn_from_gregorian(1970, 1, 1), 2_440_588);
    }

    #[test]
    fn consecutive_days_advance_all_cycles() {
        let a = MayaDate::from_gregorian(2026, 10, 18);
        let b = MayaDate::from_gregorian(2026, 10, 19);
        assert_eq!(b.day_count, a.day_count + 1);
        assert_eq!(b.tzolkin_number, a.tzolkin_number % 13 + 1);
        assert_eq!(b.tzolkin_name, (a.tzolkin_name + 1) % 20);
    }
}
