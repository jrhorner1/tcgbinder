package importer

import (
	"testing"

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/scryfall"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/scryfall/scryfalltest"
)

func TestParseImportFormats(t *testing.T) {
	tests := []struct {
		name, csv, format string
		want              Row
	}{
		{
			name:   "EchoMTG",
			format: "EchoMTG",
			csv: "inventory_id,user_id,emid,foil,condition,lang,price_acquired,date_acquired,name,set_code,expansion,collector_number,tcgplayer_id,mid,foil_price,price_change,price_updated\n" +
				"1,2,3,1.0,LP,EN,0.11,2018-08-08,Forest (266),DOM,Dominaria,266,999,100055030,0.2,0.0,2026-10-31 15:28:08\n",
			want: Row{Name: "Forest (266)", SetCode: "dom", SetName: "Dominaria", Number: "266", TCGPlayerID: 999,
				MultiverseID: 100055030, Quantity: 1, Finish: "foil", Condition: collection.ConditionLightPlay, Language: "en",
				PurchasePrice: 0.11, AcquiredDate: "2018-08-08"},
		},
		{
			name:   "Moxfield",
			format: "Moxfield",
			csv: `"Count","Tradelist Count","Name","Edition","Condition","Language","Foil","Tags","Last Modified","Collector Number","Alter","Proxy","Purchase Price"` + "\n" +
				`"3","0","Lightning Bolt","2xm","Near Mint","English","foil","","2024-01-02 10:00:00.000000","117","False","False","1.50"` + "\n",
			want: Row{Name: "Lightning Bolt", SetCode: "2xm", Number: "117", Quantity: 3, Finish: "foil",
				Condition: collection.ConditionNearMint, Language: "en", PurchasePrice: 1.5},
		},
		{
			name:   "ManaBox",
			format: "ManaBox",
			csv: "Name,Set code,Set name,Collector number,Foil,Rarity,Quantity,ManaBox ID,Scryfall ID,Purchase price,Misprint,Altered,Condition,Language,Purchase price currency\n" +
				"Sol Ring,C21,Commander 2021,263,normal,uncommon,2,1,0afa0e33-4804-4b00-b625-c2d6b61090fc,2.00,false,false,near_mint,ja,USD\n",
			want: Row{Name: "Sol Ring", SetCode: "c21", SetName: "Commander 2021", Number: "263", Quantity: 2,
				ScryfallID: "0afa0e33-4804-4b00-b625-c2d6b61090fc", Finish: "nonfoil", Condition: collection.ConditionNearMint,
				Language: "ja", PurchasePrice: 2},
		},
		{
			name:   "Deckbox",
			format: "Deckbox",
			csv: "Count,Tradelist Count,Name,Edition,Edition Code,Card Number,Condition,Language,Foil,Signed,Artist Proof,Altered Art,Misprint,Promo,Textless,My Price\n" +
				"1,0,Counterspell,Mirage,MIR,,Played,English,,,,,,,,$4.00\n",
			want: Row{Name: "Counterspell", SetCode: "mir", SetName: "Mirage", Quantity: 1, Finish: "nonfoil",
				Condition: collection.ConditionModerate, Language: "en"},
		},
		{
			name:   "TCGplayer",
			format: "TCGplayer",
			csv: "Quantity,Name,Simple Name,Set,Card Number,Set Code,Printing,Condition,Language,Rarity,Product ID,SKU\n" +
				"4,Llanowar Elves,Llanowar Elves,Dominaria,168,DOM,Normal,Near Mint,English,C,160000,123\n",
			want: Row{Name: "Llanowar Elves", SetCode: "dom", SetName: "Dominaria", Number: "168", Quantity: 4,
				TCGPlayerID: 160000, Finish: "nonfoil", Condition: collection.ConditionNearMint, Language: "en"},
		},
		{
			name:   "Dragon Shield with sep line",
			format: "Dragon Shield",
			csv: "sep=,\nFolder Name,Quantity,Trade Quantity,Card Name,Set Code,Set Name,Card Number,Condition,Printing,Language,Price Bought,Date Bought,LOW,MID,MARKET\n" +
				"Binder,1,0,Shock,M19,Core Set 2019,156,Excellent,Foil,English,0.25,2019-03-04,0.1,0.2,0.3\n",
			want: Row{Name: "Shock", SetCode: "m19", SetName: "Core Set 2019", Number: "156", Quantity: 1,
				Finish: "foil", Condition: collection.ConditionLightPlay, Language: "en", PurchasePrice: 0.25, AcquiredDate: "2019-03-04"},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rows, format, err := ParseCSV([]byte(tt.csv))
			if err != nil {
				t.Fatal(err)
			}
			if format != tt.format {
				t.Errorf("format = %q, want %q", format, tt.format)
			}
			if len(rows) != 1 {
				t.Fatalf("got %d rows, want 1", len(rows))
			}
			got := rows[0]
			got.Line = 0
			if got != tt.want {
				t.Errorf("row =\n %+v\nwant\n %+v", got, tt.want)
			}
		})
	}
}

func TestParseImportRejectsUnknownFile(t *testing.T) {
	if _, _, err := ParseCSV([]byte("foo,bar\n1,2\n")); err == nil {
		t.Fatal("expected an error for a file without card columns")
	}
}

func TestCleanCardName(t *testing.T) {
	for in, want := range map[string]string{
		"Forest (266)":                       "Forest",
		"Knight Token (001)":                 "Knight",
		"Emblem - Tezzeret, Artifice Master": "Tezzeret, Artifice Master Emblem",
		"Checklist Card - Ixalan":            "Ixalan Checklist",
		"Lightning Bolt":                     "Lightning Bolt",
	} {
		if got := cleanCardName(in); got != want {
			t.Errorf("cleanCardName(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestResolveRowsFallbacksAndValidation(t *testing.T) {
	catalog := []scryfall.Card{
		{ID: "1", Name: "Shock", Set: "m19", Number: "156", MultiverseIDs: []int{100}},
		{ID: "2", Name: "Forest", Set: "dom", Number: "266"},
		{ID: "3", Name: "Opt", Set: "dom", Number: "60", MultiverseIDs: []int{300}},
		{ID: "4", Name: "Nicol Bolas, the Ravager // Nicol Bolas, the Arisen", Set: "m19", Number: "218"},
	}
	client := scryfalltest.NewCollectionServer(t, catalog)
	rows := []Row{
		{Name: "Shock", SetCode: "m19", Number: "156"},                        // exact set+number
		{Name: "Forest (266)", SetCode: "dom", Number: "266"},                 // decorated name still validates
		{Name: "Opt", SetCode: "dom", Number: "156", MultiverseID: 300},       // wrong number → multiverse fallback
		{Name: "Opt", SetCode: "dom", Number: "999", MultiverseID: 123456789}, // bad mid must not sink the batch
		{Name: "Nicol Bolas, the Ravager", SetCode: "m19"},                    // front face name+set
		{Name: "Not A Card", SetCode: "zzz", Number: "1"},
	}
	res := Resolve(client, rows, nil)
	wantIDs := []string{"1", "2", "3", "3", "4", ""}
	wantMethods := []string{"set+number", "set+number", "multiverse", "name+set", "name+set", ""}
	for i, r := range res {
		gotID := ""
		if r.Card != nil {
			gotID = r.Card.ID
		}
		if gotID != wantIDs[i] || r.Method != wantMethods[i] {
			t.Errorf("row %d (%s): got card %q via %q, want %q via %q", i, rows[i].Name, gotID, r.Method, wantIDs[i], wantMethods[i])
		}
	}
	if !res[4].Approximate || res[0].Approximate {
		t.Errorf("name-only matches should be flagged approximate, exact ones not")
	}
}
