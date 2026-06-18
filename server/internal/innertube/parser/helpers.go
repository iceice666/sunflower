// server/internal/innertube/parser/helpers.go
package parser

import "encoding/json"

// getString traverses a nested map[string]any by path and returns the string
// value at the leaf, or "" if any step is missing or not a string.
func getString(m map[string]any, path ...string) string {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return ""
		}
		if i == len(path)-1 {
			s, _ := v.(string)
			return s
		}
		next, ok := v.(map[string]any)
		if !ok {
			return ""
		}
		cur = next
	}
	return ""
}

// getArray traverses a nested map[string]any and returns the []any at the leaf.
func getArray(m map[string]any, path ...string) []any {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return nil
		}
		if i == len(path)-1 {
			a, _ := v.([]any)
			return a
		}
		next, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		cur = next
	}
	return nil
}

// getMap traverses a nested map[string]any and returns the sub-map at the leaf.
func getMap(m map[string]any, path ...string) map[string]any {
	cur := m
	for _, key := range path {
		v, ok := cur[key]
		if !ok {
			return nil
		}
		next, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		cur = next
	}
	return cur
}

// getInt traverses a nested map[string]any and returns the int at the leaf.
func getInt(m map[string]any, path ...string) int {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return 0
		}
		if i == len(path)-1 {
			switch n := v.(type) {
			case float64:
				return int(n)
			case int:
				return n
			}
			return 0
		}
		next, ok := v.(map[string]any)
		if !ok {
			return 0
		}
		cur = next
	}
	return 0
}

// firstRunText extracts the text from the first run in a "runs" array under
// the given field name. Falls back to field.simpleText. This is needed because
// getString cannot traverse []any using numeric string keys.
func firstRunText(m map[string]any, field string) string {
	runs := getArray(m, field, "runs")
	if len(runs) > 0 {
		if r, ok := runs[0].(map[string]any); ok {
			if t := getString(r, "text"); t != "" {
				return t
			}
		}
	}
	return getString(m, field, "simpleText")
}

// unmarshalMap decodes raw JSON into a map[string]any for use with helpers.
// Returns nil on error — parsers treat nil as empty, never error.
func unmarshalMap(raw []byte) map[string]any {
	var m map[string]any
	_ = json.Unmarshal(raw, &m)
	return m
}

// arrayFirstMap returns the first element of arr as map[string]any, or nil.
// Used when a path includes a JSON array and we want the first element.
func arrayFirstMap(arr []any) map[string]any {
	if len(arr) == 0 {
		return nil
	}
	m, _ := arr[0].(map[string]any)
	return m
}
