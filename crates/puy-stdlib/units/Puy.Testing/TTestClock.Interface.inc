// ===== The virtual clock (docs/spec/stdlib/Puy.Testing/README.md chapter 6) =====

type
  // The isolate's virtual clock. Once frozen, the time the program reads (Now, TInstant.Now,
  // TStopwatch, Sleep) comes from it and moves only when the program moves it; the time the
  // runtime watches with (the time limit of `puy test`) stays real. Nothing turns it back to the
  // real clock: each test runs in an isolate of its own.
  TTestClock = record
    // Stops both clocks at the real time. On a frozen clock, does nothing.
    class procedure Freeze; overload; static;
    // Stops the clocks with the wall clock at At; the monotonic clock stops at the real time
    // (where it is, if frozen).
    class procedure Freeze(const At: TInstant); overload; static;
    // Moves the wall clock and the monotonic clock on by Milliseconds.
    class procedure Advance(Milliseconds: Int64); static;
    // Sets the wall clock to At (back too); the monotonic clock does not move.
    class procedure SetNow(const At: TInstant); static;
  end;
