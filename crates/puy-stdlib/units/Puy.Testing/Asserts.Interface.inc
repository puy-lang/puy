// ===== AssertRaises and AssertNear (docs/spec/stdlib/Puy.Testing/README.md chapters 3 and 4) =====

// Calls P and returns the exception of class E (or a descendant) it raised. Raises
// EAssertionFailed if P raised nothing, or if it raised an exception of another class (that
// exception is then the InnerException). The exception returned is reference counted: it is not
// freed by the caller.
function AssertRaises<E: Exception>(const P: TProc): E;

// Raises EAssertionFailed unless Actual and Expected are equal or differ by Tolerance at most.
// A NaN in Actual or Expected fails, and so does an infinity unless both are the same infinity; a negative or NaN Tolerance is EArgumentException. Single,
// Currency and the integers convert to Double; Extended and Real are Double. One version only, so
// that a Currency or an integer mixed with real literals is not an ambiguous call.
procedure AssertNear(const Actual, Expected, Tolerance: Double);
