package recs

// sourceAffinity scores how much the user tends to engage with a candidate's
// source, normalized to 0..1. Local library tracks score highest (the user
// curated them); YouTube tracks score lower but non-trivially. Mirrors
// Metrolist's sourceAffinity table.
var affinityTable = map[string]float64{
	"local": 1.0,
	"yt":    0.6,
}

func sourceAffinity(c Candidate) float64 {
	if v, ok := affinityTable[c.Source]; ok {
		return v
	}
	return 0.4 // unknown source — mild affinity
}
