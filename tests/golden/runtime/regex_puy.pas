program RegexPuy;

// System.RegularExpressions where Puy differs from Delphi or where Delphi was not checked
// (spec/stdlib/System.RegularExpressions/README.md; docs/delphi-diff.md): Unicode classes and
// case folding, `$` and the line breaks, empty matches and roNotEmpty, groups that take no part,
// no match, lone surrogates, positions inside a surrogate pair, the replacement of several
// matches, anonymous evaluators, the unsupported syntax and the messages of the errors.

uses
  System.SysUtils, System.RegularExpressions;

procedure Show(const A: TArray<string>);
begin
  Write(Length(A), ':');
  for var S in A do
    Write(' [', S, ']');
  WriteLn;
end;

procedure Compile(const Pattern: string);
begin
  try
    TRegEx.Create(Pattern);
    WriteLn(Pattern, ': ok');
  except
    on E: ERegularExpressionError do
      WriteLn(Pattern, ': ', E.Message);
  end;
end;

begin
  // Unicode: \w, \d, \s and \b take every script; (?i) folds by Unicode's simple case folding
  WriteLn('w ', TRegEx.IsMatch('é', '^\w$'), ' d ', TRegEx.IsMatch('１', '^\d$'),
    ' s ', TRegEx.IsMatch(#$3000, '^\s$'));
  WriteLn('b ', TRegEx.Matches('éa é', '\b\w+\b').Count);
  WriteLn('kelvin ', TRegEx.IsMatch('K', #$212A, [TRegExOption.roIgnoreCase]));
  WriteLn('ascii ', TRegEx.IsMatch('é', '(?-u:\w)'));

  // `$` is only the end (or a line's end with roMultiLine); \r, \n and \r\n end a line, and
  // `.` matches neither \r nor \n
  WriteLn('end ', TRegEx.IsMatch('abc'#10, 'c$'), ' ', TRegEx.IsMatch('abc'#10, 'c\n?\z'));
  WriteLn('dot ', TRegEx.IsMatch(#13, '^.$'), ' ', TRegEx.IsMatch(#13, '^.$', [TRegExOption.roSingleLine]));

  // Empty matches: dropped with roNotEmpty (the default of Create), found without it
  WriteLn('x* ', TRegEx.Matches('abc', 'x*').Count, ' ', TRegEx.Matches('abc', 'x*', []).Count);
  WriteLn('lazy ', TRegEx.Matches('aa', 'a*?').Count, ' ', TRegEx.Matches('aa', 'a+?').Count);
  WriteLn('lines ', TRegEx.Matches('a'#10'b', '^', [TRegExOption.roMultiLine]).Count);
  WriteLn(TRegEx.Replace('abc', 'x*', '-', []));
  Show(TRegEx.Split('abc', 'x*', []));

  // A group that takes no part, and no match
  var M := TRegEx.Match('b', '(a)|(b)');
  WriteLn(M.Groups.Count, ' ', M.Groups[1].Success, ' ', M.Groups[1].Index, ' ',
    M.Groups[1].Length, ' [', M.Groups[1].Value, '] ', M.Groups[2].Index);
  WriteLn(TRegEx.Match('a', '(a)|(b)').Groups.Count);
  Show(TRegEx.Split('a1b', '(1)|(2)'));
  WriteLn(TRegEx.Replace('b', '(a)|(b)', '[$1|$2|$+]'));
  M := TRegEx.Match('xyz', 'q');
  WriteLn(M.Success, ' ', M.Index, ' ', M.Length, ' [', M.Value, '] ', M.Groups.Count,
    ' [', M.Result('$0'), ']');
  M := M.NextMatch;
  WriteLn(M.Success);
  M := Default(TMatch);
  WriteLn(M.Success, ' ', M.NextMatch.Success);

  // The references of a replacement string in every match
  WriteLn(TRegEx.Replace('1-a 2-b 3-c', '(\d)-(?<w>\w)', '[$2$1${w}\1$&$`]'));

  // Lone surrogates outside the matches are kept; inside, a match sees U+FFFD
  var S := TRegEx.Replace('a'#$D800'b-c', '-', '+');
  WriteLn(Length(S), ' ', Ord(S[2]), ' ', Copy(S, 3, 3));
  M := TRegEx.Match('x'#$DC00'y', '.y');
  WriteLn(M.Index, ' ', M.Length, ' ', Ord(M.Value[1]));
  // A start inside a surrogate pair starts at the next character
  M := TRegEx.Create('.').Match(#$D83D#$DE00'ab', 2);
  WriteLn(M.Index, ' ', M.Value);

  // An anonymous method as the evaluator: Replace is overloaded, and the candidate's type gives
  // the method its type (semantics.md 4.7); a variable is passed as a value, not called
  WriteLn(TRegEx.Replace('a1b2', '\d',
    function(const Match: TMatch): string
    begin
      Result := '<' + Match.Value + '>';
    end));
  var N := 0;
  var Evaluator: TMatchEvaluator :=
    function(const Match: TMatch): string
    begin
      Inc(N);
      Result := IntToStr(N * 10 + StrToInt(Match.Value));
    end;
  WriteLn(TRegEx.Replace('a1b2', '\d', Evaluator));
  WriteLn(TRegEx.Create('\d').Replace('a1b2', Evaluator, 1));

  // A range that ends between the halves of a surrogate pair stops before the pair
  M := TRegEx.Create('a.?').Match('a'#$D83D#$DE00'b', 1, 2);
  WriteLn(M.Index, ' ', M.Length);
  // A collection never assigned has no matches
  var Matches := Default(TMatchCollection);
  WriteLn(Matches.Count);
  for var Each in Matches do
    WriteLn(Each.Value);

  // Split from a start position with a count, and with groups
  Show(TRegEx.Create(',').Split('a,b,c,d', 2, 3));

  // Patterns the engine does not take, and the messages
  Compile('(a)\1');
  Compile('a(?=b)');
  Compile('(?<=a)b');
  Compile('a*+');
  Compile('(?>a)');
  Compile('a\Z');
  Compile('a{');
  Compile('(a');
  Compile('[a');
  Compile('(?<n>a)(?<n>b)');
  Compile('a{,2}');
  Compile('(?P<n>a)');
  try
    WriteLn(TRegEx.Match('a', '(a)').Groups[2].Value);
  except
    on E: ERegularExpressionError do
      WriteLn(E.Message);
  end;
  try
    WriteLn(TRegEx.Match('a', '(a)').Groups['n'].Value);
  except
    on E: ERegularExpressionError do
      WriteLn(E.Message);
  end;
  try
    WriteLn(TRegEx.Matches('a', 'a')[1].Value);
  except
    on E: ERegularExpressionError do
      WriteLn(E.Message);
  end;
  try
    WriteLn(TRegEx.Create('a').IsMatch('abc', 4));
  except
    on E: ERegularExpressionError do
      WriteLn(E.Message);
  end;
  try
    WriteLn(TRegEx.Create('a').Match('abc', 2, 3).Success);
  except
    on E: ERegularExpressionError do
      WriteLn(E.Message);
  end;
  try
    WriteLn(TRegEx.Create('a').Match('', 1).Success);
  except
    on E: ERegularExpressionError do
      WriteLn(E.Message);
  end;
  WriteLn(TRegEx.Create('a').IsMatch('abc', -5));
end.
