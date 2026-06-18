package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Search builds the POST body for /youtubei/v1/search with WEB_REMIX context.
func Search(query string, locale models.Locale) map[string]any {
	body := innertube.BuildWebRemixContext(locale)
	body["query"] = query
	return body
}
