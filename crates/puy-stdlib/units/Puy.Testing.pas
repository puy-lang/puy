unit Puy.Testing;

// The helpers of the tests that `puy test` runs (docs/spec/stdlib/Puy.Testing/README.md): Skip
// to skip a test while it runs, AssertRaises to check the exception a call raises, AssertNear
// to compare real numbers within a tolerance, AssertSnapshot to compare a value with a stored
// snapshot, TTestClock to move the isolate's clock, and TMemoryFileSystem to replace its file
// system. Assert is in System. A test unit names this unit in its uses (§5.9).
//
// Split as System.Math: Name.Interface.inc holds the declarations and Name.Implementation.inc
// the bodies of one group, read only when a program names one of them. Asserts is AssertRaises
// and AssertNear; Snapshot is AssertSnapshot, whose files the runner of `puy test` reads and
// writes through Puy.Testing.Native (crates/puy-stdlib-ext); TTestClock is the virtual clock.

interface

uses
  System.SysUtils, System.Generics.Collections;

type
  // Raised by Skip: the runner reports a test that ends with it (or a descendant) as SKIP, with
  // the message as the reason.
  ESkipTest = class(Exception);

// Skips the test that is running: raises ESkipTest with Reason as its message.
procedure Skip(const Reason: string);

{$I Puy.Testing/Asserts.Interface.inc}
{$I Puy.Testing/Snapshot.Interface.inc}
{$I Puy.Testing/TTestClock.Interface.inc}
{$I Puy.Testing/TMemoryFileSystem.Interface.inc}

implementation

uses
  // The Rust part of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  Puy.Testing.Native;

procedure Skip(const Reason: string);
begin
  raise ESkipTest.Create(Reason);
end;

{$I Puy.Testing/Asserts.Implementation.inc}
{$I Puy.Testing/Snapshot.Implementation.inc}
{$I Puy.Testing/TTestClock.Implementation.inc}
{$I Puy.Testing/TMemoryFileSystem.Implementation.inc}

end.
