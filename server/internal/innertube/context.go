package innertube

import "github.com/iceice666/sunflower/server/internal/innertube/models"

const (
	androidMusicClientName    = "ANDROID_MUSIC"
	androidMusicClientVersion = "7.27.52"
	androidMusicClientID      = "21"
	androidMusicAPIKey        = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8"
	androidMusicUserAgent     = "com.google.android.apps.youtube.music/" + androidMusicClientVersion + " (Linux; U; Android 11) gzip"

	webRemixClientName    = "WEB_REMIX"
	webRemixClientVersion = "1.20230501.01.00"
	webRemixClientID      = "67"
	webRemixAPIKey        = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-NKNELL6Cg"
	webRemixUserAgent     = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
)

// BuildAndroidMusicContext returns the base POST body context for ANDROID_MUSIC
// requests (player, next). Stream URLs from this context arrive as plain URLs
// (no signatureCipher), requiring only n-param decryption.
func BuildAndroidMusicContext(locale models.Locale) map[string]any {
	return map[string]any{
		"context": map[string]any{
			"client": map[string]any{
				"clientName":        androidMusicClientName,
				"clientVersion":     androidMusicClientVersion,
				"androidSdkVersion": 30,
				"userAgent":         androidMusicUserAgent,
				"hl":                locale.HL,
				"gl":                locale.GL,
				"utcOffsetMinutes":  0,
			},
		},
	}
}

// BuildWebRemixContext returns the base POST body context for WEB_REMIX
// requests (browse, search). Stream URLs may include signatureCipher and
// require sig-cipher decryption in addition to n-param decryption.
func BuildWebRemixContext(locale models.Locale) map[string]any {
	return map[string]any{
		"context": map[string]any{
			"client": map[string]any{
				"clientName":    webRemixClientName,
				"clientVersion": webRemixClientVersion,
				"hl":            locale.HL,
				"gl":            locale.GL,
			},
		},
	}
}

// AndroidMusicAPIKey is the public API key for ANDROID_MUSIC InnerTube requests.
func AndroidMusicAPIKey() string { return androidMusicAPIKey }

// WebRemixAPIKey is the public API key for WEB_REMIX InnerTube requests.
func WebRemixAPIKey() string { return webRemixAPIKey }
