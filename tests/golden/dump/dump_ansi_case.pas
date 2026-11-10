program DumpAnsiCase;

// The hir and bytecode dumps of the Ansi* case routines (semantics.md 5.3, bytecode.md 2.16): an
// omitted option argument is the first value of its type (cmSimple, cfLower), and the comparisons
// take their three arguments in consecutive registers.

uses System.SysUtils;

var
  S: string;
begin
  S := 'Aß';
  WriteLn(AnsiUpperCase(S), AnsiLowerCase(S, TCaseMapping.cmFull));
  WriteLn(AnsiSameText(S, 'aSS'), AnsiCompareText(S, 'b', TCaseFold.cfUpper));
end.
