from __future__ import annotations

from decimal import Decimal

_ONES = [
    "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
    "Ten", "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen", "Sixteen", "Seventeen",
    "Eighteen", "Nineteen",
]
_TENS = ["", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety"]


def _under_thousand(value: int) -> str:
    parts: list[str] = []
    if value >= 100:
        parts.extend((_ONES[value // 100], "Hundred"))
        value %= 100
    if value >= 20:
        tens = _TENS[value // 10]
        parts.append(f"{tens}-{_ONES[value % 10]}" if value % 10 else tens)
    elif value:
        parts.append(_ONES[value])
    return " ".join(parts)


def _western(value: int) -> str:
    if value == 0:
        return "Zero"
    if value < 0:
        return "Minus " + _western(-value)
    parts: list[str] = []
    for scale, label in ((1_000_000_000, "Billion"), (1_000_000, "Million"), (1_000, "Thousand")):
        if value >= scale:
            count, value = divmod(value, scale)
            parts.extend((_western(count), label))
    if value:
        parts.append(_under_thousand(value))
    return " ".join(parts)


def _indian(value: int) -> str:
    if value == 0:
        return "Zero"
    if value < 0:
        return "Minus " + _indian(-value)
    parts: list[str] = []
    for scale, label in ((10_000_000, "Crore"), (100_000, "Lakh"), (1_000, "Thousand")):
        if value >= scale:
            count, value = divmod(value, scale)
            parts.extend((_western(count), label))
    if value:
        parts.append(_under_thousand(value))
    return " ".join(parts)


def money_in_words(amount: Decimal, currency: str) -> str:
    currency = currency.upper()
    quantized = amount.quantize(Decimal("0.01"))
    negative = quantized < 0
    absolute = abs(quantized)
    major = int(absolute)
    minor = int((absolute - Decimal(major)) * 100)

    if currency == "EUR":
        parts = [_western(major), "Euro" if major == 1 else "Euros"]
        if minor:
            parts.extend(("and", _western(minor), "Cent" if minor == 1 else "Cents"))
    elif currency == "INR":
        parts = [_indian(major), "Rupee" if major == 1 else "Rupees"]
        if minor:
            parts.extend(("and", _western(minor), "Paisa" if minor == 1 else "Paise"))
    else:
        parts = [_western(major), currency]
        if minor:
            parts.extend(("and", _western(minor), "Hundredths"))

    result = " ".join(parts) + " Only"
    return "Minus " + result if negative else result
