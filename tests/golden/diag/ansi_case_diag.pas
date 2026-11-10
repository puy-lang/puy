program AnsiCaseDiag;

// Diagnostics of the option argument of the Ansi* case routines (semantics.md 5.3): an
// unqualified value (E0736 with the fix that qualifies it), a value of another type (E0702), the
// argument counts (E0705), and the option types needing `uses System.SysUtils` like the routines.

uses System.SysUtils;

var
  S: string;
  B: Boolean;
begin
  S := AnsiUpperCase('a', cmFull);
  B := AnsiSameText('a', 'b', cfUpper);
  S := AnsiLowerCase('a', TCaseFold.cfUpper);
  B := AnsiSameText('a', 'b', True);
  S := AnsiUpperCase('a', 1);
  S := AnsiUpperCase;
  S := AnsiLowerCase('a', TCaseMapping.cmFull, TCaseMapping.cmFull);
  B := AnsiCompareText('a') = 0;
  WriteLn(S, B);
end.
