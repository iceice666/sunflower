package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Next builds the POST body for /youtubei/v1/next with ANDROID_MUSIC context.
// If cont is non-zero, the continuation token is included to fetch the next page.
func Next(videoID string, cont continuation.Cursor, locale models.Locale) map[string]any {
	body := innertube.BuildAndroidMusicContext(locale)
	body["videoId"] = videoID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return body
}
