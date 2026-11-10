program CaseString;

// A `string` selector (semantics.md 4.10): exact, case-sensitive matches, a list of labels, an
// empty string, a `Char` label converted to `string`, and `else`. It is not a diff/ case because
// FPC 3.2 does not compile a `case` over a `UnicodeString` in `{$mode delphiunicode}`.

var
  S: string;
  Words: TArray<string>;
begin
  Words := ['one', 'two', 'One', '', 'three', 'x', 'あ'];
  for S in Words do
    case S of
      'one': WriteLn('1');
      'two', 'three': WriteLn('2 or 3');
      '': WriteLn('empty');
      'x': WriteLn('char label');
      'あ': WriteLn('non-ASCII');
    else
      WriteLn('unknown ', S);
    end;
end.
