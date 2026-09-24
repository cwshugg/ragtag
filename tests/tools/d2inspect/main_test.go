package main

import (
	"reflect"
	"strings"
	"testing"
)

func TestInspectReportsExactCompilerFacts(t *testing.T) {
	result, err := inspect(
		"test.d2",
		strings.NewReader(
			"direction: right\n"+
				"a: \"A\\nactive\"\n"+
				"a.style.fill: \"#fff3bf\"\n"+
				"a.style.stroke: \"#e67700\"\n"+
				"a.style.stroke-width: 3\n"+
				"a.style.border-radius: 6\n"+
				"b: {\n  label: \"B\\ndone\"\n  c: \"C\\nblocked\"\n}\n"+
				"a -> b\n",
		),
	)
	if err != nil {
		t.Fatal(err)
	}
	wantNodes := []nodeFact{
		{
			ID:            "a",
			Label:         "A\nactive",
			LabelNewlines: 1,
			Shape:         "rectangle",
			Classes:       []string{},
			Style: styleFact{
				Fill:         "#fff3bf",
				Stroke:       "#e67700",
				StrokeWidth:  "3",
				BorderRadius: "6",
			},
		},
		{
			ID:            "b",
			Label:         "B\ndone",
			LabelNewlines: 1,
			Shape:         "rectangle",
			Classes:       []string{},
			Style:         styleFact{},
		},
		{
			ID:            "b.c",
			Parent:        "b",
			Label:         "C\nblocked",
			LabelNewlines: 1,
			Shape:         "rectangle",
			Classes:       []string{},
			Style:         styleFact{},
		},
	}
	if result.Direction != "right" {
		t.Fatalf("direction = %q, want right", result.Direction)
	}
	if !reflect.DeepEqual(result.Nodes, wantNodes) {
		t.Fatalf("nodes = %#v, want %#v", result.Nodes, wantNodes)
	}
	wantEdges := []edgeFact{{
		Source:      "a",
		Destination: "b",
		TargetArrow: true,
	}}
	if !reflect.DeepEqual(result.Edges, wantEdges) {
		t.Fatalf("edges = %#v, want %#v", result.Edges, wantEdges)
	}
	if result.LabelNewlines != 3 || !result.SafeStyles {
		t.Fatalf("unexpected semantic facts: %+v", result)
	}
}

func TestInspectRejectsInvalidD2(t *testing.T) {
	if _, err := inspect("test.d2", strings.NewReader("a: {")); err == nil {
		t.Fatal("expected parse failure")
	}
}

func TestInspectTreatsHostileQuotedTextAsData(t *testing.T) {
	result, err := inspect(
		"test.d2",
		strings.NewReader(`a: "@import link: \${value}"`+"\n"),
	)
	if err != nil {
		t.Fatal(err)
	}
	if result.HasImport || result.HasSubstitute || result.HasLink || result.HasClass {
		t.Fatalf("quoted data was classified as syntax: %+v", result)
	}
	if len(result.Nodes) != 1 || result.Nodes[0].Label != `@import link: ${value}` {
		t.Fatalf("quoted label was not decoded exactly: %+v", result.Nodes)
	}
}

func TestInspectRejectsNonGeneratedStyleCapabilities(t *testing.T) {
	result, err := inspect(
		"test.d2",
		strings.NewReader("a: safe\na.style.animated: true\n"),
	)
	if err != nil {
		t.Fatal(err)
	}
	if result.SafeStyles {
		t.Fatal("animated style unexpectedly accepted as generated-safe")
	}
}
