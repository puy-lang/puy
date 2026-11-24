unit System.TimeSpan;

// TTimeSpan (docs/spec/stdlib/System.TimeSpan/README.md). The type is declared by the part of
// System written in Pascal (docs/spec/stdlib/System/TTimeSpan.md), which every file sees without
// a `uses`, so that TInstant can take and give it; this unit names the same type for the
// programs that write `uses System.TimeSpan`.

interface

type
  TTimeSpan = System.TTimeSpan;

implementation

end.
