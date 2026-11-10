// The option types of the Ansi* case routines are names of System.SysUtils (semantics.md 5.3):
// without `uses System.SysUtils`, E0700 with the fix that adds it.

var M: TCaseMapping := TCaseMapping.cmFull;
WriteLn(Ord(M));
