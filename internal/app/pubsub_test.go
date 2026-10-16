package app_test

import (
	"context"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/core"
)

func recv(t *testing.T, ch <-chan core.Event) core.Event {
	t.Helper()
	select {
	case e, ok := <-ch:
		if !ok {
			t.Fatal("channel closed")
		}
		return e
	case <-time.After(2 * time.Second):
		t.Fatal("no event")
	}
	return core.Event{}
}

func closedSoon(t *testing.T, ch <-chan core.Event) {
	t.Helper()
	deadline := time.After(2 * time.Second)
	for {
		select {
		case _, ok := <-ch:
			if !ok {
				return
			}
		case <-deadline:
			t.Fatal("channel not closed")
		}
	}
}

func TestBrokerFanOutStampsSeqAndTime(t *testing.T) {
	clk := apptest.NewFakeClock(time.Time{})
	b := app.NewBroker(clk)
	ctx := t.Context()
	a, c := b.Subscribe(ctx), b.Subscribe(ctx)
	if b.Subscribers() != 2 {
		t.Fatalf("Subscribers = %d", b.Subscribers())
	}

	b.Publish(core.Event{Name: "x", Data: 1})
	b.Publish(core.Event{Name: "y"})
	for _, ch := range []<-chan core.Event{a, c} {
		e1, e2 := recv(t, ch), recv(t, ch)
		if e1.Name != "x" || e1.Data != 1 || e2.Name != "y" {
			t.Fatalf("got %+v %+v", e1, e2)
		}
		if e1.Seq != 1 || e2.Seq != 2 {
			t.Fatalf("Seq = %d, %d", e1.Seq, e2.Seq)
		}
		if !e1.At.Equal(apptest.Epoch) {
			t.Fatalf("At = %v, want clock time", e1.At)
		}
	}

	at := time.Date(2020, 1, 1, 0, 0, 0, 0, time.UTC)
	b.Publish(core.Event{Name: "z", At: at})
	if e := recv(t, a); !e.At.Equal(at) || e.Seq != 3 {
		t.Fatalf("explicit At must be kept: %+v", e)
	}
}

func TestBrokerNilClockUsesWallTime(t *testing.T) {
	b := app.NewBroker(nil)
	ch := b.Subscribe(t.Context())
	before := time.Now()
	b.Publish(core.Event{Name: "x"})
	if e := recv(t, ch); e.At.Before(before) {
		t.Fatalf("At = %v", e.At)
	}
}

func TestBrokerSlowSubscriberIsCutWithoutBlocking(t *testing.T) {
	b := app.NewBroker(nil)
	slow := b.Subscribe(t.Context()) // never read until the end
	fast := b.Subscribe(t.Context())

	total := app.SubscriberBuffer + 10
	done := make(chan []uint64)
	go func() {
		var got []uint64
		for range total {
			b.Publish(core.Event{Name: "tick"})
			// The fast subscriber keeps up; Publish never waits for the slow one.
			e := <-fast
			got = append(got, e.Seq)
		}
		done <- got
	}()
	var got []uint64
	select {
	case got = <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("Publish blocked on a slow subscriber")
	}
	for i, s := range got {
		if s != uint64(i+1) {
			t.Fatalf("fast subscriber: Seq not contiguous at %d: %v", i, s)
		}
	}

	// The slow one got its full buffer, then was closed.
	n := 0
	for range slow {
		n++
	}
	if n != app.SubscriberBuffer {
		t.Fatalf("slow subscriber got %d events, want %d before the cut", n, app.SubscriberBuffer)
	}
	if b.Subscribers() != 1 {
		t.Fatalf("Subscribers = %d after the cut", b.Subscribers())
	}
}

func TestBrokerSeqMonotonicUnderConcurrency(t *testing.T) {
	b := app.NewBroker(nil)
	ch := b.Subscribe(t.Context())
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for range 20 {
				b.Publish(core.Event{Name: "p"})
			}
		}()
	}
	wg.Wait()
	var last uint64
	for range 160 {
		e := recv(t, ch)
		if e.Seq <= last {
			t.Fatalf("Seq went %d → %d", last, e.Seq)
		}
		last = e.Seq
	}
}

func TestBrokerUnsubscribeOnContextCancel(t *testing.T) {
	b := app.NewBroker(nil)
	ctx, cancel := context.WithCancel(t.Context())
	ch := b.Subscribe(ctx)
	cancel()
	closedSoon(t, ch)
	deadline := time.Now().Add(2 * time.Second)
	for b.Subscribers() != 0 {
		if time.Now().After(deadline) {
			t.Fatal("subscriber not removed")
		}
		time.Sleep(time.Millisecond)
	}
	b.Publish(core.Event{Name: "after"}) // must not panic on a closed channel
}

func TestBrokerClose(t *testing.T) {
	b := app.NewBroker(nil)
	ch := b.Subscribe(context.Background())
	b.Close()
	closedSoon(t, ch)
	b.Close() // idempotent
	b.Publish(core.Event{Name: "x"})
	closedSoon(t, b.Subscribe(t.Context()))
	if b.Subscribers() != 0 {
		t.Fatal("subscribers after Close")
	}
}
