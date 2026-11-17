unit System.SysUtils;

// The part of System.SysUtils written in Pascal. The rest of the unit (IntToStr, Format,
// TReplaceFlags, the built-in exception classes, ...) is built into the compiler; a `uses`
// of the unit makes both visible (docs/spec/collections.md 1.1, 3.2).

interface

type
  // Raised by the containers of System.Generics.Collections: a duplicate key, an empty stack
  // or queue.
  EListError = class(Exception);

  // A key that is not in a dictionary (Puy only; caught by `on E: EListError` too).
  EKeyNotFound = class(EListError);

implementation

end.
