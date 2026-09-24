// d2inspect compiles D2 source with the exact grammar and semantic model used by Ragtag CI.
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"reflect"
	"sort"
	"strings"

	"github.com/d2lang/d2/d2ast"
	"github.com/d2lang/d2/d2compiler"
	"github.com/d2lang/d2/d2graph"
	"github.com/d2lang/d2/d2parser"
)

const maximumInputBytes = 256 * 1024 * 1024

type styleFact struct {
	Fill         string `json:"fill,omitempty"`
	Stroke       string `json:"stroke,omitempty"`
	StrokeWidth  string `json:"stroke_width,omitempty"`
	BorderRadius string `json:"border_radius,omitempty"`
	Opacity      string `json:"opacity,omitempty"`
	StrokeDash   string `json:"stroke_dash,omitempty"`
}

type nodeFact struct {
	ID            string    `json:"id"`
	Parent        string    `json:"parent,omitempty"`
	Label         string    `json:"label"`
	LabelNewlines int       `json:"label_newlines"`
	Shape         string    `json:"shape,omitempty"`
	Classes       []string  `json:"classes"`
	Style         styleFact `json:"style"`
}

type edgeFact struct {
	Source      string `json:"source"`
	Destination string `json:"destination"`
	SourceArrow bool   `json:"source_arrow"`
	TargetArrow bool   `json:"target_arrow"`
	Label       string `json:"label,omitempty"`
}

type inspection struct {
	Direction     string     `json:"direction"`
	Nodes         []nodeFact `json:"nodes"`
	Edges         []edgeFact `json:"edges"`
	LabelNewlines int        `json:"label_newlines"`
	HasImport     bool       `json:"has_import"`
	HasSubstitute bool       `json:"has_substitution"`
	HasLink       bool       `json:"has_link"`
	HasClass      bool       `json:"has_class"`
	SafeStyles    bool       `json:"safe_styles"`
}

func inspect(path string, input io.Reader) (inspection, error) {
	limited := io.LimitReader(input, maximumInputBytes+1)
	source, err := io.ReadAll(limited)
	if err != nil {
		return inspection{}, fmt.Errorf("read input: %w", err)
	}
	if len(source) > maximumInputBytes {
		return inspection{}, fmt.Errorf("input exceeds %d bytes", maximumInputBytes)
	}

	ast, err := d2parser.Parse(path, strings.NewReader(string(source)), nil)
	if err != nil {
		return inspection{}, fmt.Errorf("parse D2: %w", err)
	}
	graph, _, err := d2compiler.Compile(path, strings.NewReader(string(source)), nil)
	if err != nil {
		return inspection{}, fmt.Errorf("compile D2: %w", err)
	}

	result := inspection{
		Direction:  graph.Root.Direction.Value,
		Nodes:      make([]nodeFact, 0, len(graph.Objects)),
		Edges:      make([]edgeFact, 0, len(graph.Edges)),
		SafeStyles: true,
	}
	d2ast.Walk(ast, func(node d2ast.Node) bool {
		switch node.(type) {
		case *d2ast.Import:
			result.HasImport = true
		case *d2ast.Substitution:
			result.HasSubstitute = true
		}
		return true
	})
	for _, object := range graph.Objects {
		parent := ""
		if object.Parent != nil && object.Parent.Parent != nil {
			parent = object.Parent.AbsID()
		}
		fact := nodeFact{
			ID:            object.AbsID(),
			Parent:        parent,
			Label:         object.Label.Value,
			LabelNewlines: strings.Count(object.Label.Value, "\n"),
			Shape:         object.Shape.Value,
			Classes:       append([]string{}, object.Classes...),
			Style:         generatedStyle(object.Style),
		}
		sort.Strings(fact.Classes)
		result.Nodes = append(result.Nodes, fact)
		result.LabelNewlines += fact.LabelNewlines
		result.HasLink = result.HasLink || object.Link != nil
		result.HasClass = result.HasClass || len(object.Classes) > 0
		result.SafeStyles = result.SafeStyles && hasOnlyGeneratedNodeStyles(object.Style)
	}
	for _, edge := range graph.Edges {
		result.Edges = append(result.Edges, edgeFact{
			Source:      edge.Src.AbsID(),
			Destination: edge.Dst.AbsID(),
			SourceArrow: edge.SrcArrow,
			TargetArrow: edge.DstArrow,
			Label:       edge.Label.Value,
		})
		result.HasLink = result.HasLink || edge.Link != nil
		result.SafeStyles = result.SafeStyles && styleIsEmpty(edge.Style)
	}
	sort.Slice(result.Nodes, func(i, j int) bool {
		return result.Nodes[i].ID < result.Nodes[j].ID
	})
	sort.Slice(result.Edges, func(i, j int) bool {
		left := result.Edges[i]
		right := result.Edges[j]
		if left.Source != right.Source {
			return left.Source < right.Source
		}
		if left.Destination != right.Destination {
			return left.Destination < right.Destination
		}
		return left.Label < right.Label
	})
	return result, nil
}

func generatedStyle(style d2graph.Style) styleFact {
	return styleFact{
		Fill:         scalarValue(style.Fill),
		Stroke:       scalarValue(style.Stroke),
		StrokeWidth:  scalarValue(style.StrokeWidth),
		BorderRadius: scalarValue(style.BorderRadius),
		Opacity:      scalarValue(style.Opacity),
		StrokeDash:   scalarValue(style.StrokeDash),
	}
}

func scalarValue(value *d2graph.Scalar) string {
	if value == nil {
		return ""
	}
	return value.Value
}

func hasOnlyGeneratedNodeStyles(style d2graph.Style) bool {
	allowed := map[string]bool{
		"Opacity":      true,
		"Stroke":       true,
		"Fill":         true,
		"StrokeWidth":  true,
		"StrokeDash":   true,
		"BorderRadius": true,
	}
	value := reflect.ValueOf(style)
	kind := value.Type()
	for index := 0; index < value.NumField(); index++ {
		if !value.Field(index).IsNil() && !allowed[kind.Field(index).Name] {
			return false
		}
	}
	return true
}

func styleIsEmpty(style d2graph.Style) bool {
	value := reflect.ValueOf(style)
	for index := 0; index < value.NumField(); index++ {
		if !value.Field(index).IsNil() {
			return false
		}
	}
	return true
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: d2inspect FILE")
		os.Exit(2)
	}
	file, err := os.Open(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	defer file.Close()
	result, err := inspect(os.Args[1], file)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := json.NewEncoder(os.Stdout).Encode(result); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
