program StringHelperPuy;

// TStringHelper (spec/stdlib/System/TStringHelper.md) where Puy decides the result: empty
// strings and separators, indexes out of the string (cut to it, as Copy does), splitting with
// a trailing separator and the first matching separator, the Unicode case mapping, Join and the
// other class functions, the explicit `TStringHelper.M(X)` form, and Chars out of range
// (ERangeError with 0-based bounds). The cases Delphi agrees with are in diff/system.

procedure ShowParts(const Parts: TArray<string>);
begin
  Write(Length(Parts), ':');
  for var P in Parts do
    Write(' [', P, ']');
  WriteLn;
end;

begin
  var S := 'abcabc';
  var E := '';
  // Empty values
  WriteLn(E.IsEmpty, ' ', E.Length, ' ', E.IndexOf('a'), ' ', S.IndexOf(''), ' ', S.LastIndexOf(''));
  WriteLn(S.Contains(''), ' ', S.StartsWith(''), ' ', S.EndsWith(''), ' ', E.StartsWith('a'));
  ShowParts(E.Split([',']));
  ShowParts(S.Split([]));
  ShowParts(S.Split(['']));
  // Indexes out of the string
  WriteLn(S.IndexOf('a', -5), ' ', S.IndexOf('a', 1), ' ', S.IndexOf('a', 100), ' ', S.IndexOf('c', 0, 2));
  WriteLn(S.IndexOf('bc', 1, 2), ' ', S.IndexOf('bc', 1, 1), ' ', S.IndexOf('a', MaxInt, MaxInt));
  WriteLn(S.LastIndexOf('a', 2), ' ', S.LastIndexOf('bc', 2), ' ', S.LastIndexOf('bc', 1), ' ', S.LastIndexOf('a', 100));
  WriteLn(S.IndexOfAny(['c'], 3), ' ', S.LastIndexOfAny(['a'], 2), ' ', S.IndexOfAny([]), ' ', S.LastIndexOfAny(['z']));
  WriteLn(S.LastIndexOf('a', Low(Integer)), ' ', S.LastIndexOfAny(['a'], Low(Integer)), ' ', S.IndexOfAny(['a'], Low(Integer)), ' ', S.IndexOf('a', Low(Integer), MaxInt));
  WriteLn('[', S.Substring(-2, 2), '][', S.Substring(4), '][', S.Substring(6), '][', S.Substring(MaxInt), ']');
  WriteLn('[', S.Substring(1, 0), '][', S.Substring(1, -3), '][', S.Substring(5, MaxInt), ']');
  WriteLn('[', S.Insert(-1, '<'), '][', S.Insert(100, '>'), '][', S.Remove(-1), '][', S.Remove(10), ']');
  WriteLn('[', S.Remove(-1, 2), '][', S.Remove(4, 100), '][', S.Remove(2, 0), ']');
  WriteLn('[', S.PadLeft(-1), '][', E.PadRight(2, '*'), ']');
  // Splitting: every separator found makes one more part, empty ones included
  ShowParts(('a,b,').Split([',']));
  ShowParts((',,').Split([',']));
  ShowParts(('a--b-c').Split(['-', '--']));
  ShowParts(('a--b-c').Split(['--', '-']));
  ShowParts(('Hello, World').Split([', ']));
  ShowParts(('a,b,c,d,e,f,g,h,i,j').Split([',']));
  ShowParts(('abab').Split(['abc', 'b']));
  // The Unicode case mapping of AnsiUpperCase / AnsiLowerCase
  WriteLn(('straße ǅ').ToUpper, ' ', ('ÀÉÎ Σ').ToLower);
  WriteLn(('ÉCOLE').StartsWith('éc', True), ' ', ('ÉCOLE').EndsWith('olE', True), ' ', ('ÉCOLE').StartsWith('éc'));
  // Class functions and the other members
  WriteLn(TStringHelper.Join(', ', ['a', 'b', 'c']), '|', TStringHelper.Join('-', []), '|', TStringHelper.Join('-', ['x']));
  WriteLn(TStringHelper.IsNullOrEmpty(E), ' ', TStringHelper.IsNullOrEmpty(S));
  WriteLn(TStringHelper.Compare('a', 'B'), ' ', TStringHelper.Compare('a', 'B', True), ' ', TStringHelper.Compare('b', 'B', True));
  WriteLn(('bb').CompareTo('ba'), ' ', S.CompareTo(S), ' ', ('Ab').CompareTo('ab'));
  WriteLn(('it''s').QuotedString, ' ', ('say "hi"').QuotedString('"'), ' ', E.QuotedString);
  WriteLn(S.CountChar('b'), ' ', S.CountChar('z'), ' ', Length(E.ToCharArray));
  WriteLn(('-9000000000').ToInt64, ' ', ('2.5').ToDouble:0:2, ' ', TStringHelper.Trim(' x '), '|', TStringHelper.Substring(S, 3));
  // Chars and the conversions raise
  try
    WriteLn(S.Chars[-1]);
  except
    on X: ERangeError do WriteLn(X.ClassName, ': ', X.Message);
  end;
  try
    WriteLn(('12x').ToInteger);
  except
    on X: EConvertError do WriteLn(X.ClassName, ': ', X.Message);
  end;
  WriteLn(E.Chars[0]);
end.
