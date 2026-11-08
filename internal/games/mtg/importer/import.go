// Package importer reads Magic collection exports from other apps (EchoMTG, ManaBox,
// Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and generic CSV) and matches
// each row to its exact Scryfall printing.
package importer

import "strings"

// ChooseFinish keeps the exported finish when the printing exists in it; otherwise it
// falls back to the printing's only finish (e.g. a foil-only promo marked nonfoil).
func ChooseFinish(want string, available []string) string {
	if len(available) == 0 {
		return want
	}
	for _, f := range available {
		if f == want {
			return want
		}
	}
	if len(available) == 1 {
		return available[0]
	}
	return want
}

// ParseFinish normalizes foil markers from any export to Scryfall's finish names.
func ParseFinish(raw string) string {
	clean := strings.ToLower(strings.TrimSpace(raw))
	switch {
	case strings.Contains(clean, "etched"):
		return "etched"
	case strings.Contains(clean, "non"), strings.Contains(clean, "normal"), strings.Contains(clean, "regular"):
		return "nonfoil"
	case strings.Contains(clean, "foil"):
		return "foil"
	}
	switch clean {
	case "1", "1.0", "true", "yes", "y", "f", "x":
		return "foil"
	}
	return "nonfoil"
}

var languageCodes = map[string]string{
	"en": "en", "english": "en",
	"es": "es", "sp": "es", "spanish": "es",
	"fr": "fr", "french": "fr",
	"de": "de", "german": "de",
	"it": "it", "italian": "it",
	"pt": "pt", "portuguese": "pt", "portuguese (brazil)": "pt",
	"ja": "ja", "jp": "ja", "japanese": "ja",
	"ko": "ko", "kr": "ko", "korean": "ko",
	"ru": "ru", "russian": "ru",
	"zhs": "zhs", "cs": "zhs", "zh": "zhs", "simplified chinese": "zhs", "chinese simplified": "zhs", "chinese": "zhs",
	"zht": "zht", "ct": "zht", "traditional chinese": "zht", "chinese traditional": "zht",
	"he": "he", "hebrew": "he", "la": "la", "latin": "la", "grc": "grc", "ancient greek": "grc",
	"ar": "ar", "arabic": "ar", "sa": "sa", "sanskrit": "sa", "ph": "ph", "phyrexian": "ph",
}

// ParseLanguage maps language names/codes to Scryfall's codes, defaulting to English.
func ParseLanguage(raw string) string {
	if code, ok := languageCodes[strings.ToLower(strings.TrimSpace(raw))]; ok {
		return code
	}
	return "en"
}
