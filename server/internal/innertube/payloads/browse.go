package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Browse builds the POST body for /youtubei/v1/browse with WEB_REMIX context.
func Browse(browseID string, cont continuation.Cursor, locale models.Locale) map[string]any {
	body := innertube.BuildWebRemixContext(locale)
	body["browseId"] = browseID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return body
}
