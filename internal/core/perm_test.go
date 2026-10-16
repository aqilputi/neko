package core

import (
	"reflect"
	"testing"
)

func TestPermModes(t *testing.T) {
	want := []string{"", "acceptEdits", "auto", "plan", "bypassPermissions"}
	got := PermModes()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("PermModes = %q", got)
	}
	got[1] = "x" // a copy: the caller cannot change the list
	if PermModes()[1] != "acceptEdits" {
		t.Fatal("PermModes must return a copy")
	}
	if PermBypass != "bypassPermissions" {
		t.Fatal(PermBypass)
	}
	for _, m := range want {
		if !ValidPermMode(m) {
			t.Errorf("%q must be valid", m)
		}
	}
	if ValidPermMode("yolo") || ValidPermMode("Plan") {
		t.Error("unknown modes are invalid")
	}
}
