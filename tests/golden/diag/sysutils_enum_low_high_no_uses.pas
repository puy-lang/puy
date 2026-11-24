// The enumerated types of System.SysUtils passed to Low / High (spec/types.md chapter 7) are
// names of System.SysUtils: without `uses System.SysUtils`, E0700 with the fix that adds it.

WriteLn(Low(TReplaceFlag));
WriteLn(High(TIOErrorKind));
