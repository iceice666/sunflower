package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Player builds the POST body for /youtubei/v1/player with ANDROID_MUSIC context.
// The "params" value "CgIQBg==" requests audio-only formats.
func Player(videoID string, locale models.Locale) map[string]any {
	body := innertube.BuildAndroidMusicContext(locale)
	body["videoId"] = videoID
	body["params"] = "CgIQBg=="
	return body
}
