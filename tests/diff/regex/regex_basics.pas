program RegexBasics;

// System.RegularExpressions (spec/stdlib/System.RegularExpressions/README.md) where Puy and
// Delphi agree: TRegEx.Create, IsMatch, Match / NextMatch, Matches, the groups by number and by
// name, the positions in UTF-16 code units, the options, Escape and the class of the errors
// (caught as Exception: Delphi XE4 declares ERegularExpressionError in another unit). FPC 3.2.2
// has no System.RegularExpressions, so the expectation is written by hand and checked with
// Delphi (check-dcc; the case is on record-fpc's list of cases FPC cannot compile).
// What differs from Delphi is in golden/runtime/regex_puy.

uses
  System.SysUtils, System.RegularExpressions;

var
  R: TRegEx;
  M: TMatch;
  G: TGroup;
  I: Integer;
begin
  R := TRegEx.Create('(\d+)-(?<w>[a-z]+)');
  WriteLn(R.IsMatch('x 12-ab y'));
  WriteLn(R.IsMatch('x 12- y'));
  M := R.Match('x 12-ab y 345-cd');
  WriteLn(M.Success, ' ', M.Index, ' ', M.Length, ' ', M.Value);
  WriteLn(M.Groups.Count);
  for I := 0 to M.Groups.Count - 1 do
    WriteLn(I, ' ', M.Groups[I].Success, ' ', M.Groups[I].Index, ' ', M.Groups[I].Length,
      ' [', M.Groups[I].Value, ']');
  WriteLn('named ', M.Groups['w'].Value, ' ', M.Groups['w'].Index);
  M := M.NextMatch;
  WriteLn(M.Success, ' ', M.Index, ' ', M.Value, ' ', M.Groups[1].Value);
  M := M.NextMatch;
  WriteLn(M.Success, ' [', M.Value, '] ', M.Groups.Count);
  for M in R.Matches('1-a 2-b 3-c') do
    WriteLn('m ', M.Index, ' ', M.Value);
  WriteLn(R.Matches('1-a 2-b').Count, ' ', R.Matches('1-a 2-b')[1].Value);
  WriteLn(R.Matches('none').Count);
  for G in R.Match('7-z').Groups do
    WriteLn('g ', G.Value);

  // The class functions
  WriteLn(TRegEx.IsMatch('abc', 'B'));
  WriteLn(TRegEx.IsMatch('abc', 'B', [TRegExOption.roIgnoreCase]));
  WriteLn(TRegEx.Match('abcb', 'b').Index);
  WriteLn(TRegEx.Matches('abcb', 'b').Count);

  // Positions count UTF-16 code units: a surrogate pair is two
  M := TRegEx.Match(#$D83D#$DE00'ab', 'b');
  WriteLn('pair ', M.Index, ' ', M.Length);
  M := TRegEx.Match(#$D83D#$DE00'ab', '.');
  WriteLn('pair2 ', M.Index, ' ', M.Length);
  M := TRegEx.Match('日本語のテキスト', 'テ(キ)');
  WriteLn('kana ', M.Index, ' ', M.Length, ' ', M.Groups[1].Index);
  WriteLn('han ', TRegEx.IsMatch('漢', '^\p{Han}$'));
  WriteLn('caseless ', TRegEx.IsMatch(#$00C9'COLE', #$00E9'cole', [TRegExOption.roIgnoreCase]));

  // From a start position, and within a range
  R := TRegEx.Create('\w');
  WriteLn(R.Match('abcd', 3).Index);
  WriteLn(TRegEx.Create('^\w').IsMatch('ab', 2));
  WriteLn(TRegEx.Create('\bb').Match('abcd', 2).Success);
  M := R.Match('abcd', 2, 2);
  WriteLn(M.Index, ' ', M.Value);
  M := M.NextMatch;
  WriteLn(M.Index, ' ', M.Value);
  M := M.NextMatch;
  WriteLn(M.Success);
  WriteLn(TRegEx.Create('c$').Match('abcd', 2, 2).Success);
  WriteLn(TRegEx.Create('d$').Match('abcd', 2, 2).Success);
  WriteLn(TRegEx.Create('\bb').Matches('a b b', 3).Count);
  WriteLn(TRegEx.Create('a').IsMatch('abc', 0));

  // Options
  WriteLn('m ', TRegEx.Matches('a'#10'a'#13#10'a'#13'a', '^a$', [TRegExOption.roMultiLine]).Count);
  WriteLn('s ', TRegEx.IsMatch('a'#10'b', 'a.b'), ' ',
    TRegEx.IsMatch('a'#10'b', 'a.b', [TRegExOption.roSingleLine]));
  WriteLn('x ', TRegEx.IsMatch('ab', 'a b # comment', [TRegExOption.roIgnorePatternSpace]));
  WriteLn('n ', TRegEx.Match('ab', '(a)(?<n>b)', [TRegExOption.roExplicitCapture]).Groups.Count);
  WriteLn('c ', TRegEx.IsMatch('ab', 'b', [TRegExOption.roCompiled]));

  // Escape
  WriteLn(TRegEx.Escape('a.b*c+?d(e)[f]{g}|h^$\i/j-k#l m'));
  WriteLn(TRegEx.Escape('a.b*c?d', True));
  WriteLn(TRegEx.IsMatch('1+1=2', TRegEx.Escape('1+1')));

  // Errors
  try
    M := TRegEx.Match('a', '(a)');
    WriteLn(M.Groups[2].Value);
  except
    on E: Exception do WriteLn('group index ', E.ClassName);
  end;
  try
    M := TRegEx.Match('a', '(a)');
    WriteLn(M.Groups['zz'].Value);
  except
    on E: Exception do WriteLn('group name ', E.ClassName);
  end;
  try
    WriteLn(TRegEx.Matches('a', 'a')[1].Value);
  except
    on E: Exception do WriteLn('match index ', E.ClassName);
  end;
  try
    WriteLn(TRegEx.Match('a', 'b').Groups[0].Value);
  except
    on E: Exception do WriteLn('no match has no groups ', E.ClassName);
  end;
  try
    WriteLn(TRegEx.Create('a').IsMatch('abc', 4));
  except
    on E: Exception do WriteLn('start position ', E.ClassName);
  end;
  try
    WriteLn(TRegEx.Create('a').Match('abc', 2, 3).Success);
  except
    on E: Exception do WriteLn('length ', E.ClassName);
  end;
  try
    WriteLn(TRegEx.IsMatch('a', '\'));
  except
    on E: Exception do WriteLn('pattern ', E.ClassName);
  end;
end.
