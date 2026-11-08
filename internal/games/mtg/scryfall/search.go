package scryfall

import (
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"unicode"
)

// ImageURL returns the card's image, falling back to the front face for double-faced cards.
func (sc *Card) ImageURL() string {
	if sc.ImageURIs.Normal != "" {
		return sc.ImageURIs.Normal
	}
	for _, f := range sc.CardFaces {
		if f.ImageURIs.Normal != "" {
			return f.ImageURIs.Normal
		}
	}
	return ""
}

// Text returns the rules text, joining faces for multi-faced cards.
func (sc *Card) Text() string {
	if sc.OracleText != "" || len(sc.CardFaces) == 0 {
		return sc.OracleText
	}
	parts := make([]string, 0, len(sc.CardFaces))
	for _, f := range sc.CardFaces {
		parts = append(parts, f.Name+"\n"+f.OracleText)
	}
	return strings.Join(parts, "\n\n")
}

type SearchResponse struct {
	Object     string `json:"object"`
	TotalCards int    `json:"total_cards"`
	HasMore    bool   `json:"has_more"`
	NextPage   string `json:"next_page"`
	Data       []Card `json:"data"`
}

// SearchPage is one page of /cards/search results.
type SearchPage struct {
	Cards    []Card
	Total    int
	NextPage string // full URL of the next page, "" when this is the last
}

// BuildSearchQuery combines free text with an optional set code into Scryfall syntax.
// Every printing is returned (unique=prints is set by SearchCards), so a set code
// narrows the results to that set's printing(s).
func BuildSearchQuery(text, setCode string) (string, error) {
	text = strings.TrimSpace(text)
	setCode = strings.ToLower(strings.TrimSpace(setCode))
	if setCode != "" && !isSetCodeInput(setCode) {
		return "", fmt.Errorf("%q is not a set code (letters and digits, e.g. M19)", setCode)
	}
	switch {
	case text == "" && setCode == "":
		return "", fmt.Errorf("enter a card name or a set code")
	case setCode == "":
		return text, nil
	case text == "":
		return "set:" + setCode, nil
	}
	return text + " set:" + setCode, nil
}

// SearchCards runs a Scryfall search returning every printing, ordered by name and
// then newest first. Pass the previous page's NextPage to continue; a search with no
// matches returns an empty page, not an error.
func (c *Client) SearchCards(query, nextPage string) (SearchPage, error) {
	return c.search(url.Values{"q": {query}, "unique": {"prints"}, "order": {"name"}}, nextPage)
}

// SearchAll returns every printing matching query in collector-number order, following
// all result pages (up to maxPages of 175 cards). Extras and variations are included,
// so a set query lists every card Scryfall counts in the set.
func (c *Client) SearchAll(query string, maxPages int) ([]Card, error) {
	params := url.Values{"q": {query}, "unique": {"prints"}, "order": {"set"},
		"include_extras": {"true"}, "include_variations": {"true"}}
	var out []Card
	next := ""
	for page := 0; page < maxPages; page++ {
		res, err := c.search(params, next)
		if err != nil {
			return nil, err
		}
		out = append(out, res.Cards...)
		if res.NextPage == "" {
			break
		}
		next = res.NextPage
	}
	return out, nil
}

func (c *Client) search(params url.Values, nextPage string) (SearchPage, error) {
	path := "/cards/search?" + params.Encode()
	if nextPage != "" {
		if !strings.HasPrefix(nextPage, c.BaseURL+"/") {
			return SearchPage{}, fmt.Errorf("unexpected next page URL %q", nextPage)
		}
		path = strings.TrimPrefix(nextPage, c.BaseURL)
	}
	var resp SearchResponse
	status, err := c.Do("GET", path, nil, &resp, true)
	if status == http.StatusNotFound {
		return SearchPage{}, nil // Scryfall answers 404 when nothing matches
	}
	if err != nil {
		return SearchPage{}, err
	}
	page := SearchPage{Cards: resp.Data, Total: resp.TotalCards}
	if resp.HasMore {
		page.NextPage = resp.NextPage
	}
	return page, nil
}

func isSetCodeInput(s string) bool {
	if len(s) < 2 || len(s) > 8 {
		return false
	}
	for _, r := range s {
		if !unicode.IsLetter(r) && !unicode.IsDigit(r) {
			return false
		}
	}
	return true
}

// GetCard fetches one printing by Scryfall ID.
func (c *Client) GetCard(id string) (*Card, error) {
	if !IsUUID(id) {
		return nil, fmt.Errorf("%q is not a Scryfall ID", id)
	}
	var card Card
	if _, err := c.Do("GET", "/cards/"+id, nil, &card, false); err != nil {
		return nil, err
	}
	return &card, nil
}

// CardByNumber fetches the printing with a set code and collector number, in a
// language (Scryfall code, e.g. "ja"; "" for English). found is false on a 404.
func (c *Client) CardByNumber(set, number, lang string) (card *Card, found bool, err error) {
	path := "/cards/" + url.PathEscape(strings.ToLower(set)) + "/" + url.PathEscape(number)
	if lang != "" && lang != "en" {
		path += "/" + url.PathEscape(lang)
	}
	var sc Card
	status, err := c.Do("GET", path, nil, &sc, false)
	if status == http.StatusNotFound {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	return &sc, true, nil
}

// NamedFuzzy finds a card by an approximate name, tolerating typos (Scryfall's
// /cards/named?fuzzy=). found is false when nothing (or more than one card) matches.
func (c *Client) NamedFuzzy(name string) (card *Card, found bool, err error) {
	var sc Card
	status, err := c.Do("GET", "/cards/named?fuzzy="+url.QueryEscape(name), nil, &sc, false)
	if status == http.StatusNotFound {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	return &sc, true, nil
}
