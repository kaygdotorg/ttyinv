use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CurrencyWordsCapability {
    pub code: &'static str,
    pub grouping: &'static str,
    pub exponent: u32,
    pub major_unit: &'static str,
    pub minor_unit: Option<&'static str>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CurrencyWords {
    pub code: &'static str,
    pub major_singular: &'static str,
    pub major_plural: &'static str,
    pub minor_singular: Option<&'static str>,
    pub minor_plural: Option<&'static str>,
}

const DEFAULT: CurrencyWords = CurrencyWords {
    code: "",
    major_singular: "Unit",
    major_plural: "Units",
    minor_singular: Some("Minor Unit"),
    minor_plural: Some("Minor Units"),
};

pub fn currency_words(currency: &str) -> CurrencyWords {
    match currency {
        "AED" => CurrencyWords {
            code: "AED",
            major_singular: "Dirham",
            major_plural: "Dirhams",
            minor_singular: Some("Fils"),
            minor_plural: Some("Fils"),
        },
        "AUD" => CurrencyWords {
            code: "AUD",
            major_singular: "Dollar",
            major_plural: "Dollars",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "CAD" => CurrencyWords {
            code: "CAD",
            major_singular: "Dollar",
            major_plural: "Dollars",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "NZD" => CurrencyWords {
            code: "NZD",
            major_singular: "Dollar",
            major_plural: "Dollars",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "SGD" => CurrencyWords {
            code: "SGD",
            major_singular: "Dollar",
            major_plural: "Dollars",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "BHD" => CurrencyWords {
            code: "BHD",
            major_singular: "Dinar",
            major_plural: "Dinars",
            minor_singular: Some("Fils"),
            minor_plural: Some("Fils"),
        },
        "CHF" => CurrencyWords {
            code: "CHF",
            major_singular: "Franc",
            major_plural: "Francs",
            minor_singular: Some("Centime"),
            minor_plural: Some("Centimes"),
        },
        "CNY" => CurrencyWords {
            code: "CNY",
            major_singular: "Yuan",
            major_plural: "Yuan",
            minor_singular: Some("Fen"),
            minor_plural: Some("Fen"),
        },
        "DKK" => CurrencyWords {
            code: "DKK",
            major_singular: "Krone",
            major_plural: "Kroner",
            minor_singular: Some("Ore"),
            minor_plural: Some("Ore"),
        },
        "NOK" => CurrencyWords {
            code: "NOK",
            major_singular: "Krone",
            major_plural: "Kroner",
            minor_singular: Some("Ore"),
            minor_plural: Some("Ore"),
        },
        "SEK" => CurrencyWords {
            code: "SEK",
            major_singular: "Krone",
            major_plural: "Kroner",
            minor_singular: Some("Ore"),
            minor_plural: Some("Ore"),
        },
        "GBP" => CurrencyWords {
            code: "GBP",
            major_singular: "Pound",
            major_plural: "Pounds",
            minor_singular: Some("Penny"),
            minor_plural: Some("Pence"),
        },
        "HKD" => CurrencyWords {
            code: "HKD",
            major_singular: "Dollar",
            major_plural: "Dollars",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "EUR" => CurrencyWords {
            code: "EUR",
            major_singular: "Euro",
            major_plural: "Euros",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "INR" => CurrencyWords {
            code: "INR",
            major_singular: "Rupee",
            major_plural: "Rupees",
            minor_singular: Some("Paisa"),
            minor_plural: Some("Paise"),
        },
        "IQD" => CurrencyWords {
            code: "IQD",
            major_singular: "Dinar",
            major_plural: "Dinars",
            minor_singular: Some("Fils"),
            minor_plural: Some("Fils"),
        },
        "JOD" => CurrencyWords {
            code: "JOD",
            major_singular: "Dinar",
            major_plural: "Dinars",
            minor_singular: Some("Fils"),
            minor_plural: Some("Fils"),
        },
        "KWD" => CurrencyWords {
            code: "KWD",
            major_singular: "Dinar",
            major_plural: "Dinars",
            minor_singular: Some("Fils"),
            minor_plural: Some("Fils"),
        },
        "LYD" => CurrencyWords {
            code: "LYD",
            major_singular: "Dinar",
            major_plural: "Dinars",
            minor_singular: Some("Dirham"),
            minor_plural: Some("Dirhams"),
        },
        "OMR" => CurrencyWords {
            code: "OMR",
            major_singular: "Rial",
            major_plural: "Rials",
            minor_singular: Some("Baisa"),
            minor_plural: Some("Baisa"),
        },
        "TND" => CurrencyWords {
            code: "TND",
            major_singular: "Dinar",
            major_plural: "Dinars",
            minor_singular: Some("Millime"),
            minor_plural: Some("Millimes"),
        },
        "JPY" => CurrencyWords {
            code: "JPY",
            major_singular: "Yen",
            major_plural: "Yen",
            minor_singular: None,
            minor_plural: None,
        },
        "KRW" => CurrencyWords {
            code: "KRW",
            major_singular: "Won",
            major_plural: "Won",
            minor_singular: None,
            minor_plural: None,
        },
        "PKR" => CurrencyWords {
            code: "PKR",
            major_singular: "Rupee",
            major_plural: "Rupees",
            minor_singular: Some("Paisa"),
            minor_plural: Some("Paise"),
        },
        "LKR" => CurrencyWords {
            code: "LKR",
            major_singular: "Rupee",
            major_plural: "Rupees",
            minor_singular: Some("Paisa"),
            minor_plural: Some("Paise"),
        },
        "RUB" => CurrencyWords {
            code: "RUB",
            major_singular: "Ruble",
            major_plural: "Rubles",
            minor_singular: Some("Kopek"),
            minor_plural: Some("Kopeks"),
        },
        "SAR" => CurrencyWords {
            code: "SAR",
            major_singular: "Riyal",
            major_plural: "Riyals",
            minor_singular: Some("Halala"),
            minor_plural: Some("Halalas"),
        },
        "THB" => CurrencyWords {
            code: "THB",
            major_singular: "Baht",
            major_plural: "Baht",
            minor_singular: Some("Satang"),
            minor_plural: Some("Satang"),
        },
        "TRY" => CurrencyWords {
            code: "TRY",
            major_singular: "Lira",
            major_plural: "Lira",
            minor_singular: Some("Kurus"),
            minor_plural: Some("Kurus"),
        },
        "USD" => CurrencyWords {
            code: "USD",
            major_singular: "Dollar",
            major_plural: "Dollars",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        "ZAR" => CurrencyWords {
            code: "ZAR",
            major_singular: "Rand",
            major_plural: "Rand",
            minor_singular: Some("Cent"),
            minor_plural: Some("Cents"),
        },
        _ => DEFAULT,
    }
}
pub fn currency_capabilities() -> Vec<CurrencyWordsCapability> {
    const CODES: [&str; 25] = [
        "AED", "AUD", "BHD", "CAD", "CHF", "CNY", "DKK", "EUR", "GBP", "HKD", "INR", "IQD", "JOD",
        "JPY", "KWD", "LKR", "LYD", "NOK", "NZD", "OMR", "PKR", "SAR", "SEK", "USD", "ZAR",
    ];
    CODES
        .into_iter()
        .map(|code| {
            let words = currency_words(code);
            CurrencyWordsCapability {
                code,
                grouping: if code == "INR" {
                    "indian"
                } else {
                    "international"
                },
                exponent: crate::currency_exponent(code),
                major_unit: words.major_plural,
                minor_unit: words.minor_plural,
            }
        })
        .collect()
}

const ONES: [&str; 20] = [
    "Zero",
    "One",
    "Two",
    "Three",
    "Four",
    "Five",
    "Six",
    "Seven",
    "Eight",
    "Nine",
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];
const TENS: [&str; 10] = [
    "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
];

fn under_thousand(value: u128) -> String {
    debug_assert!(value < 1000);
    if value < 20 {
        return ONES[value as usize].into();
    }
    if value < 100 {
        let tens = TENS[(value / 10) as usize];
        let ones = value % 10;
        return if ones == 0 {
            tens.into()
        } else {
            format!("{tens}-{}", ONES[ones as usize])
        };
    }
    let hundreds = value / 100;
    let remainder = value % 100;
    if remainder == 0 {
        format!("{} Hundred", ONES[hundreds as usize])
    } else {
        format!(
            "{} Hundred {}",
            ONES[hundreds as usize],
            under_thousand(remainder)
        )
    }
}

fn international(value: u128) -> String {
    if value < 1000 {
        return under_thousand(value);
    }
    const SCALES: [&str; 7] = [
        "",
        "Thousand",
        "Million",
        "Billion",
        "Trillion",
        "Quadrillion",
        "Quintillion",
    ];
    let mut groups = Vec::new();
    let mut n = value;
    let mut scale = 0;
    while n > 0 {
        let group = n % 1000;
        if group != 0 {
            let text = under_thousand(group);
            groups.push(if scale == 0 {
                text
            } else {
                format!("{text} {}", SCALES[scale])
            });
        }
        n /= 1000;
        scale += 1;
    }
    groups.into_iter().rev().collect::<Vec<_>>().join(" ")
}

fn indian(value: u128) -> String {
    if value < 1000 {
        return under_thousand(value);
    }
    let mut groups = Vec::new();
    let first = value % 1000;
    let mut n = value / 1000;
    if first != 0 {
        groups.push(under_thousand(first));
    }
    const SCALES: [&str; 9] = [
        "Thousand",
        "Lakh",
        "Crore",
        "Arab",
        "Kharab",
        "Neel",
        "Padma",
        "Shankh",
        "Mahashankh",
    ];
    let mut scale = 0;
    while n > 0 {
        let group = n % 100;
        if group != 0 {
            groups.push(format!("{} {}", under_thousand(group), SCALES[scale]));
        }
        n /= 100;
        scale += 1;
    }
    groups.into_iter().rev().collect::<Vec<_>>().join(" ")
}

fn unit_name<'a>(singular: &'a str, plural: &'a str, value: u128) -> &'a str {
    if value == 1 {
        singular
    } else {
        plural
    }
}

/// Converts an exact document amount into title-cased words using the currency's grouping.
/// Values are rounded using the same minor-unit policy as rendered money.
pub fn amount_in_words(amount: Decimal, currency: &str) -> String {
    let exponent = crate::currency_exponent(currency);
    let rounded = amount.round_dp_with_strategy(exponent, RoundingStrategy::MidpointNearestEven);
    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let scale = Decimal::from(10u64.pow(exponent));
    let minor_total = (rounded.abs() * scale).trunc().to_u128().unwrap_or(0);
    let divisor = 10u128.pow(exponent);
    let major = minor_total / divisor;
    let minor = minor_total % divisor;
    let info = currency_words(currency);
    let number = if currency == "INR" {
        indian(major)
    } else {
        international(major)
    };
    let major_name = unit_name(info.major_singular, info.major_plural, major);
    let mut result = if negative {
        "Negative ".to_owned()
    } else {
        String::new()
    };
    result.push_str(&number);
    result.push(' ');
    result.push_str(major_name);
    if exponent > 0 {
        let minor_name = unit_name(
            info.minor_singular.unwrap_or("Minor Unit"),
            info.minor_plural.unwrap_or("Minor Units"),
            minor,
        );
        let minor_words = if currency == "INR" {
            indian(minor)
        } else {
            international(minor)
        };
        result.push_str(" and ");
        result.push_str(&minor_words);
        result.push(' ');
        result.push_str(minor_name);
    }
    result.push_str(" Only");
    result
}

#[cfg(test)]
mod tests {
    use super::amount_in_words;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn d(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    #[test]
    fn invoice_examples_use_currency_grouping() {
        assert_eq!(
            amount_in_words(d("5250.90"), "EUR"),
            "Five Thousand Two Hundred Fifty Euros and Ninety Cents Only"
        );
        assert_eq!(
            amount_in_words(d("560499.97"), "INR"),
            "Five Lakh Sixty Thousand Four Hundred Ninety-Nine Rupees and Ninety-Seven Paise Only"
        );
    }

    #[test]
    fn handles_zero_three_and_negative() {
        assert_eq!(amount_in_words(d("0"), "JPY"), "Zero Yen Only");
        assert_eq!(
            amount_in_words(d("24.691"), "KWD"),
            "Twenty-Four Dinars and Six Hundred Ninety-One Fils Only"
        );
        assert_eq!(
            amount_in_words(d("-15.50"), "EUR"),
            "Negative Fifteen Euros and Fifty Cents Only"
        );
    }

    #[test]
    fn uses_minor_unit_names_and_international_grouping() {
        assert_eq!(
            amount_in_words(d("1000000.01"), "USD"),
            "One Million Dollars and One Cent Only"
        );
        assert_eq!(
            amount_in_words(d("12.34"), "GBP"),
            "Twelve Pounds and Thirty-Four Pence Only"
        );
    }
}
