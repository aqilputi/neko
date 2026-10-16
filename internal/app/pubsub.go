package app

import (
	"context"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// SubscriberBuffer is how many events a subscriber may lag behind before it
// is cut off.
const SubscriberBuffer = 256

// Broker fans events out to subscribers. Publish never blocks: a subscriber
// whose buffer is full is cut (its channel closed) and must resubscribe.
type Broker struct {
	clock Clock

	mu     sync.Mutex
	seq    uint64
	subs   map[*subscriber]struct{}
	closed bool
}

type subscriber struct {
	ch   chan core.Event
	gone chan struct{} // closed when the subscriber is removed
}

// NewBroker stamps events with clock (wall time if nil).
func NewBroker(clock Clock) *Broker {
	if clock == nil {
		clock = wallClock{}
	}
	return &Broker{clock: clock, subs: map[*subscriber]struct{}{}}
}

// Publish stamps e with the next Seq (and At, if unset) and delivers it.
func (b *Broker) Publish(e core.Event) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.closed {
		return
	}
	b.seq++
	e.Seq = b.seq
	if e.At.IsZero() {
		e.At = b.clock.Now()
	}
	for s := range b.subs {
		select {
		case s.ch <- e:
		default:
			b.removeLocked(s) // too slow: cut it rather than block everyone
		}
	}
}

// Subscribe returns a channel of every event published from now on. It is
// closed when ctx ends, when the subscriber falls SubscriberBuffer behind, or
// when the broker closes.
func (b *Broker) Subscribe(ctx context.Context) <-chan core.Event {
	s := &subscriber{ch: make(chan core.Event, SubscriberBuffer), gone: make(chan struct{})}
	b.mu.Lock()
	if b.closed {
		b.mu.Unlock()
		close(s.ch)
		return s.ch
	}
	b.subs[s] = struct{}{}
	b.mu.Unlock()
	go func() {
		select {
		case <-ctx.Done():
			b.mu.Lock()
			b.removeLocked(s)
			b.mu.Unlock()
		case <-s.gone:
		}
	}()
	return s.ch
}

// removeLocked drops s once; b.mu must be held.
func (b *Broker) removeLocked(s *subscriber) {
	if _, ok := b.subs[s]; !ok {
		return
	}
	delete(b.subs, s)
	close(s.ch)
	close(s.gone)
}

// Subscribers is how many subscribers are attached.
func (b *Broker) Subscribers() int {
	b.mu.Lock()
	defer b.mu.Unlock()
	return len(b.subs)
}

// Close ends every subscription; later publishes are dropped.
func (b *Broker) Close() {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.closed = true
	for s := range b.subs {
		b.removeLocked(s)
	}
}

type wallClock struct{}

func (wallClock) Now() time.Time { return time.Now() }
