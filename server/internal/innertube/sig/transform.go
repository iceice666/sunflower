package sig

import "fmt"

type opKind int

const (
	opReverse opKind = iota
	opSplice
	opSwap
)

// Op is a single sig-cipher transformation.
type Op struct {
	Kind opKind
	Arg  int
}

// OpKindFromString converts a fixture string to opKind.
func OpKindFromString(s string) opKind {
	switch s {
	case "reverse":
		return opReverse
	case "splice":
		return opSplice
	case "swap":
		return opSwap
	default:
		panic(fmt.Sprintf("sig: unknown op kind %q", s))
	}
}

// Apply runs ops over sig in sequence and returns the result.
func Apply(ops []Op, s string) string {
	b := []byte(s)
	for _, op := range ops {
		switch op.Kind {
		case opReverse:
			for i, j := 0, len(b)-1; i < j; i, j = i+1, j-1 {
				b[i], b[j] = b[j], b[i]
			}
		case opSplice:
			if op.Arg < len(b) {
				b = b[op.Arg:]
			}
		case opSwap:
			if op.Arg < len(b) {
				b[0], b[op.Arg] = b[op.Arg], b[0]
			}
		}
	}
	return string(b)
}
