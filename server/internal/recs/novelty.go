package recs

// noveltyCap is the impression count at which a candidate is considered fully
// "seen" — novelty bottoms out at 0. Below the cap, novelty degrades linearly.
const noveltyCap = 5

// novelty returns 1 - hits/cap, clamped to [0,1]: an unseen candidate scores
// 1.0, one seen `cap` or more times scores 0. impressions24h is the number of
// times the candidate was shown in the recent impression window.
func novelty(impressions24h int) float64 {
	if impressions24h <= 0 {
		return 1
	}
	if impressions24h >= noveltyCap {
		return 0
	}
	return 1 - float64(impressions24h)/float64(noveltyCap)
}
