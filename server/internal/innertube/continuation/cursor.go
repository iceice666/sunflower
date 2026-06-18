package continuation

// Cursor is an opaque continuation token extracted from a YT response.
// It is posted back verbatim as the "continuation" field in the next request.
// Never inspect or transform the contents.
type Cursor []byte

func (c Cursor) IsZero() bool { return len(c) == 0 }
