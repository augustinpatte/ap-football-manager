//! Reference tables the save does not carry: nation codes, club cities, short codes.
//! Versioned beside the code (`config/*.csv`), each row with its source.

use crate::Result;
use ofm_core::nations::{nation_by_code, nation_by_name};
use std::collections::{BTreeMap, BTreeSet};

const CLUB_CITY_CSV: &str = include_str!("../config/club_city.csv");
const NATION_ALIASES_CSV: &str = include_str!("../config/nation_aliases.csv");

#[derive(Debug, Clone)]
pub struct Refs {
    /// club.id -> (club name as checked against world.db, city)
    cities: BTreeMap<i64, (String, String)>,
    /// FM nation name -> OFM code, for names the OFM catalogue spells differently.
    aliases: BTreeMap<String, String>,
}

impl Refs {
    pub fn embedded() -> Result<Self> {
        Self::from_csv(CLUB_CITY_CSV, NATION_ALIASES_CSV)
    }

    pub fn from_csv(city_csv: &str, alias_csv: &str) -> Result<Self> {
        let mut cities = BTreeMap::new();
        let mut reader = csv::Reader::from_reader(city_csv.as_bytes());
        for record in reader.records() {
            let record = record.map_err(|e| format!("club_city.csv: {e}"))?;
            let id: i64 = record[0]
                .parse()
                .map_err(|_| format!("club_city.csv: bad club_id {}", &record[0]))?;
            if cities
                .insert(id, (record[1].to_string(), record[2].to_string()))
                .is_some()
            {
                return Err(format!("club_city.csv: duplicate club_id {id}"));
            }
        }
        let mut aliases = BTreeMap::new();
        let mut reader = csv::Reader::from_reader(alias_csv.as_bytes());
        for record in reader.records() {
            let record = record.map_err(|e| format!("nation_aliases.csv: {e}"))?;
            let code = record[1].to_string();
            if nation_by_code(&code).is_none() {
                return Err(format!(
                    "nation_aliases.csv: {} -> {code} is not an OFM nation code",
                    &record[0]
                ));
            }
            aliases.insert(record[0].to_string(), code);
        }
        Ok(Self { cities, aliases })
    }

    /// The city of a club, checked against the club name in world.db: a mismatch means the CSV row
    /// belongs to another club (wrong id) and is reported by the caller.
    pub fn city(&self, club_id: i64) -> Option<(&str, &str)> {
        self.cities
            .get(&club_id)
            .map(|(name, city)| (name.as_str(), city.as_str()))
    }

    /// OFM country code for an FM nation name. `None` for an unnamed or unknown nation.
    pub fn nation_code(&self, fm_name: &str) -> Option<String> {
        if let Some(code) = self.aliases.get(fm_name) {
            return Some(code.clone());
        }
        nation_by_name(fm_name).map(|n| n.code.to_string())
    }
}

/// Unique 2-5 letter codes for clubs, generated deterministically (sorted by club id) from the
/// FM short name then the full name. Display only; marked `générée` in the report.
#[derive(Debug, Default)]
pub struct ShortCodes {
    used: BTreeSet<String>,
}

impl ShortCodes {
    pub fn assign(&mut self, short_name: &str, full_name: &str) -> String {
        let mut candidates: Vec<String> = Vec::new();
        let short_words = words(short_name);
        let full_words = words(full_name);
        if (2..=5).contains(&short_name.chars().count())
            && short_name
                .chars()
                .all(|c| c.is_alphabetic() && c.is_uppercase())
        {
            candidates.push(strip(short_name));
        }
        if short_words.len() >= 2 {
            let mut c = first_n(&short_words[0], 1);
            c.push_str(&first_n(&short_words[1], 2));
            candidates.push(c);
        }
        if let Some(first) = short_words.first() {
            candidates.push(first_n(first, 3));
        }
        if let Some(distinctive) = full_words.first() {
            candidates.push(first_n(distinctive, 3));
        }
        if full_words.len() >= 2 {
            let initials: String = full_words
                .iter()
                .filter_map(|w| w.chars().next())
                .take(3)
                .collect();
            candidates.push(initials);
        }
        if let Some(first) = short_words.first() {
            candidates.push(first_n(first, 4));
        }
        if let Some(distinctive) = full_words.first() {
            candidates.push(first_n(distinctive, 4));
        }
        for candidate in candidates {
            if candidate.chars().count() >= 2 && self.used.insert(candidate.clone()) {
                return candidate;
            }
        }
        let base = short_words
            .first()
            .map(|w| first_n(w, 2))
            .unwrap_or_else(|| "XX".to_string());
        for n in 2.. {
            let candidate = format!("{base}{n}");
            if self.used.insert(candidate.clone()) {
                return candidate;
            }
        }
        unreachable!("the numeric suffix space is unbounded")
    }
}

const STOP_WORDS: [&str; 22] = [
    "FC", "AFC", "AS", "SC", "SV", "VFL", "VFB", "TSV", "SSC", "AC", "US", "RC", "CD", "CF", "UD",
    "RCD", "SD", "AD", "DSC", "SG", "SPVGG", "THE",
];

/// Uppercase ASCII letters of `text`, accents folded.
fn strip(text: &str) -> String {
    text.chars()
        .filter_map(fold)
        .filter(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_uppercase()
}

fn fold(c: char) -> Option<char> {
    Some(match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'ç' | 'Ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' | 'Ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ß' => 's',
        other => other,
    })
}

fn first_n(word: &str, n: usize) -> String {
    word.chars().take(n).collect()
}

/// Significant uppercase ASCII words of a club name (stop words and numbers dropped).
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(strip)
        .filter(|w| w.len() >= 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_references_parse_and_cover_240_clubs() {
        let refs = Refs::embedded().unwrap();
        assert_eq!(refs.cities.len(), 240);
        assert_eq!(refs.city(498).unwrap().1, "Paris");
    }

    #[test]
    fn nation_codes_use_the_catalogue_and_the_aliases() {
        let refs = Refs::embedded().unwrap();
        assert_eq!(refs.nation_code("France").as_deref(), Some("FR"));
        assert_eq!(refs.nation_code("England").as_deref(), Some("ENG"));
        assert_eq!(refs.nation_code("Wales").as_deref(), Some("WAL"));
        assert_eq!(refs.nation_code("USA").as_deref(), Some("US"));
        assert_eq!(
            refs.nation_code("Republic of Ireland").as_deref(),
            Some("IE")
        );
        assert_eq!(refs.nation_code("Turkey").as_deref(), Some("TR"));
        assert_eq!(refs.nation_code("Atlantis"), None);
    }

    #[test]
    fn short_codes_are_unique_and_readable() {
        let mut codes = ShortCodes::default();
        let city = codes.assign("Man City", "Manchester City");
        let utd = codes.assign("Man Utd", "Manchester United");
        let psg = codes.assign("PSG", "Paris Saint-Germain");
        let arsenal = codes.assign("Arsenal", "Arsenal");
        assert_eq!(city, "MCI");
        assert_eq!(utd, "MUT");
        assert_eq!(psg, "PSG");
        assert_eq!(arsenal, "ARS");
        let again = codes.assign("Arsenal", "Arsenal");
        assert_ne!(again, "ARS", "a second club never reuses a code");
    }

    #[test]
    fn short_codes_fold_accents() {
        let mut codes = ShortCodes::default();
        assert_eq!(codes.assign("Köln", "1. FC Köln"), "KOL");
    }
}
