program StringHelperCase;

// The members of TStringHelper (spec/stdlib/System/TStringHelper.md) in the cases where Puy
// and Delphi agree (FPC has no helper for UnicodeString): the 0-based searches, the tests of
// the start and the end, the parts of a string, splitting on separators found inside the
// string, the case of ASCII letters, trimming and padding, with the members and overloads
// Delphi XE4 has (it has no ToInt64 and splits on characters only). Empty strings,
// out-of-range indexes, string separators, ToInt64 and the errors are in
// golden/runtime/string_helper_puy.

uses
  System.SysUtils;

procedure ShowParts(const Parts: TArray<string>);
var
  I: Integer;
begin
  Write(Length(Parts), ':');
  for I := 0 to High(Parts) do
    Write(' [', Parts[I], ']');
  WriteLn;
end;

var
  S: string;
  T: string;
  U: string;
begin
  S := 'Hello, World';
  T := '  padded text  ';
  U := '';
  // Searching
  WriteLn(S.IndexOf('o'), ' ', S.IndexOf('o', 5), ' ', S.IndexOf('xyz'));
  WriteLn(S.LastIndexOf('o'), ' ', S.LastIndexOf('l'), ' ', S.LastIndexOf('xyz'));
  WriteLn(S.IndexOfAny(['W', 'o']), ' ', S.LastIndexOfAny(['l', 'H']));
  WriteLn(S.Contains('World'), ' ', S.Contains('world'));
  WriteLn(S.StartsWith('Hello'), ' ', S.StartsWith('World'), ' ', S.EndsWith('World'));
  WriteLn(S.StartsWith('hello', True), ' ', S.EndsWith('WORLD', True), ' ', S.EndsWith('Hello'));
  // Parts of the string
  WriteLn(S.Substring(7), '|', S.Substring(0, 5), '|', S.Substring(3, 2));
  // Delphi XE4's Insert also changes the receiver, so it is called on a copy
  U := S;
  WriteLn(U.Insert(5, '!!'), '|', S.Remove(5), '|', S.Remove(5, 2));
  WriteLn(S.Replace('l', 'L'), '|', S.Replace('World', 'There'));
  WriteLn(S.Chars[0], S.Chars[7], ' ', S.Length, ' ', S.IsEmpty);
  // Splitting
  ShowParts(S.Split([',']));
  ShowParts(('a,b;c').Split([',', ';']));
  ShowParts(('one  two').Split([' ']));
  // Case, trimming and padding
  WriteLn(S.ToUpper, '|', S.ToLower);
  WriteLn('[', T.Trim, '][', T.TrimLeft, '][', T.TrimRight, ']');
  WriteLn('[', ('xxabcxx').Trim(['x']), '][', ('xxabcxx').TrimLeft(['x']), '][',
    ('xxabcxx').TrimRight(['x']), ']');
  WriteLn('[', ('42').PadLeft(5), '][', ('42').PadLeft(5, '0'), '][', ('42').PadRight(5, '.'), ']');
  WriteLn('[', S.PadLeft(3), ']');
  // Conversions and comparisons
  WriteLn(('123').ToInteger + 1, ' ', ('-42').ToInteger);
  WriteLn(S.Equals('Hello, World'), ' ', S.Equals('hello, world'));
  WriteLn(S.CompareTo('Hello, World') = 0, ' ', S.CompareTo('Zebra') < 0, ' ', S.CompareTo('Apple') > 0);
  WriteLn(Length(S.ToCharArray), ' ', S.ToCharArray[4]);
end.
