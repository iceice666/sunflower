package library

import (
	"context"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

// Scanner walks music directories and watches for file changes.
type Scanner struct {
	pool    *pgxpool.Pool
	dataDir string
	log     zerolog.Logger

	mu      sync.Mutex
	watcher *fsnotify.Watcher
}

// NewScanner creates a Scanner.
func NewScanner(pool *pgxpool.Pool, dataDir string, log zerolog.Logger) *Scanner {
	return &Scanner{pool: pool, dataDir: dataDir, log: log}
}

// Scan walks each root, calling UpsertTrack for every audio file found.
// progress receives a running count after each file; may be nil.
// Returns the total number of audio files encountered.
func (s *Scanner) Scan(ctx context.Context, roots []string, progress chan<- int) (int, error) {
	var count int
	for _, root := range roots {
		err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
			if err != nil || d.IsDir() {
				return err
			}
			if !IsAudioFile(path) {
				return nil
			}
			if ctx.Err() != nil {
				return ctx.Err()
			}
			if procErr := s.processFile(ctx, path); procErr != nil {
				s.log.Warn().Err(procErr).Str("path", path).Msg("scan: skipped")
			}
			count++
			if progress != nil {
				progress <- count
			}
			return nil
		})
		if err != nil {
			return count, err
		}
	}
	return count, nil
}

// Watch starts a recursive fsnotify watcher on roots with a 2 s debounce.
// Events from subdirectories are included. New subdirectories are added
// automatically. Replaces any previously started watcher.
func (s *Scanner) Watch(ctx context.Context, roots []string) error {
	w, err := fsnotify.NewWatcher()
	if err != nil {
		return err
	}

	s.mu.Lock()
	if s.watcher != nil {
		_ = s.watcher.Close()
	}
	s.watcher = w
	s.mu.Unlock()

	// Add each root and all its subdirectories.
	for _, root := range roots {
		s.addDirTree(w, root)
	}

	pending := make(map[string]*time.Timer)
	var pmu sync.Mutex

	go func() {
		defer w.Close()
		for {
			select {
			case <-ctx.Done():
				return
			case event, ok := <-w.Events:
				if !ok {
					return
				}
				// New directory: watch it recursively.
				if event.Has(fsnotify.Create) {
					if info, statErr := os.Stat(event.Name); statErr == nil && info.IsDir() {
						s.addDirTree(w, event.Name)
						continue
					}
				}
				if !IsAudioFile(event.Name) {
					continue
				}
				if event.Has(fsnotify.Write) || event.Has(fsnotify.Create) {
					path := event.Name
					pmu.Lock()
					if t, exists := pending[path]; exists {
						t.Reset(2 * time.Second)
					} else {
						pending[path] = time.AfterFunc(2*time.Second, func() {
							pmu.Lock()
							delete(pending, path)
							pmu.Unlock()
							if procErr := s.processFile(ctx, path); procErr != nil {
								s.log.Warn().Err(procErr).Str("path", path).Msg("watch: process failed")
							}
						})
					}
					pmu.Unlock()
				}
			case watchErr, ok := <-w.Errors:
				if !ok {
					return
				}
				s.log.Warn().Err(watchErr).Msg("watcher error")
			}
		}
	}()

	return nil
}

// addDirTree adds dir and all subdirectories to the watcher.
func (s *Scanner) addDirTree(w *fsnotify.Watcher, dir string) {
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err == nil && d.IsDir() {
			if addErr := w.Add(path); addErr != nil {
				s.log.Warn().Err(addErr).Str("dir", path).Msg("watch: add dir failed")
			}
		}
		return nil
	})
}

func (s *Scanner) processFile(ctx context.Context, path string) error {
	meta, err := ExtractTags(path)
	if err != nil {
		return err
	}
	if meta.Picture != nil {
		if artErr := SaveCoverArt(meta.Picture.Data, meta.AlbumMediaID, s.dataDir); artErr != nil {
			s.log.Warn().Err(artErr).Str("album", meta.AlbumMediaID).Msg("cover art: skipped")
		}
	}
	return UpsertTrack(ctx, s.pool, meta)
}
