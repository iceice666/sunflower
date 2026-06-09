package jobs

import (
	"context"
	"sync"

	"github.com/iceice666/sunflower/server/internal/library"
)

// RunScanJob drives a library scan, updating reg as files are processed.
// Starts the fsnotify watcher on the same roots after a successful scan.
// Must be called in a goroutine; uses ctx for cancellation.
func RunScanJob(ctx context.Context, reg *Registry, scanner *library.Scanner, jobID string, roots []string) {
	_ = reg.Update(jobID, func(j *Job) { j.Status = StatusRunning })

	progress := make(chan int, 64)
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		for count := range progress {
			_ = reg.Update(jobID, func(j *Job) { j.ProcessedFiles = count })
		}
	}()

	count, err := scanner.Scan(ctx, roots, progress)
	close(progress)
	wg.Wait() // ensure all intermediate updates are flushed

	if err != nil {
		_ = reg.Update(jobID, func(j *Job) {
			j.Status = StatusFailed
			j.Error = err.Error()
			j.ProcessedFiles = count // authoritative final count
		})
		return
	}

	_ = reg.Update(jobID, func(j *Job) {
		j.Status = StatusCompleted
		j.ProcessedFiles = count // authoritative final count
	})

	// Watch the same roots for future file changes.
	_ = scanner.Watch(ctx, roots)
}
