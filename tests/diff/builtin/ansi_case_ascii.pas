program AnsiCaseAscii;

// AnsiUpperCase / AnsiLowerCase / AnsiSameText / AnsiCompareText (semantics.md 5.3) on ASCII,
// where FPC gives the same results: they agree with UpperCase / LowerCase / SameText /
// CompareText. Non-ASCII letters, the option argument, the -1 / 0 / 1 of AnsiCompareText and
// the order of characters between `Z` and `a` are in golden/runtime/ansi_case.

uses System.SysUtils;

var
  S: string;
begin
  S := 'Hello, World 123 [x]';
  WriteLn(AnsiUpperCase(S));
  WriteLn(AnsiLowerCase(S));
  WriteLn(AnsiUpperCase(S) = UpperCase(S), ' ', AnsiLowerCase(S) = LowerCase(S));
  WriteLn(AnsiUpperCase(''), '|', AnsiLowerCase(''), '|');
  WriteLn(AnsiSameText('ABC', 'abc'), ' ', AnsiSameText('abc', 'abd'), ' ', AnsiSameText('', ''),
    ' ', AnsiSameText('ab', 'abc'), ' ', AnsiSameText('Pascal', 'pASCAL'));
  WriteLn(AnsiCompareText('abc', 'ABD') < 0, ' ', AnsiCompareText('ABC', 'abc') = 0, ' ',
    AnsiCompareText('b', 'A') > 0, ' ', AnsiCompareText('abc', 'AB') > 0, ' ',
    AnsiCompareText('', 'a') < 0, ' ', AnsiCompareText('Zeta', 'alpha') > 0);
end.
