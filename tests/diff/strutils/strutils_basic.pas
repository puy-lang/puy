program StrUtilsBasic;

// The routines of System.StrUtils on ASCII text: the head, the tail and the inside of a string,
// PosEx, the searches in a list of strings, replacing, cutting, repeating and reversing, and the
// string IfThen (docs/spec/stdlib/System.StrUtils/README.md).

uses
  System.SysUtils, System.StrUtils;

begin
  WriteLn(StartsStr('He', 'Hello'), ' ', StartsStr('he', 'Hello'), ' ', StartsText('he', 'Hello'),
    ' ', StartsStr('', 'Hello'), ' ', StartsStr('Hello!', 'Hello'));
  WriteLn(EndsStr('lo', 'Hello'), ' ', EndsStr('LO', 'Hello'), ' ', EndsText('LO', 'Hello'),
    ' ', EndsText('xHello', 'Hello'));
  WriteLn(ContainsStr('Hello', 'ell'), ' ', ContainsStr('Hello', 'ELL'), ' ',
    ContainsText('Hello', 'ELL'), ' ', ContainsStr('Hello', ''), ' ', ContainsText('', ''));
  WriteLn(AnsiStartsStr('a', 'abc'), ' ', AnsiStartsText('A', 'abc'), ' ', AnsiEndsStr('c', 'abc'),
    ' ', AnsiEndsText('C', 'abc'), ' ', AnsiContainsStr('abc', 'b'), ' ',
    AnsiContainsText('abc', 'B'));

  WriteLn(PosEx('na', 'banana'), ' ', PosEx('na', 'banana', 4), ' ', PosEx('na', 'banana', 6),
    ' ', PosEx('a', 'banana', 6), ' ', PosEx('a', 'banana', 7));
  WriteLn(PosEx('a', 'banana', 0), ' ', PosEx('', 'banana', 1),
    ' ', PosEx('x', 'banana'), ' ', PosEx('a', '', 1));

  WriteLn(IndexStr('b', ['a', 'b', 'b']), ' ', IndexStr('B', ['a', 'b']), ' ',
    IndexText('B', ['a', 'b']), ' ', IndexStr('x', ['a']), ' ', IndexStr('', ['a', '']));
  WriteLn(MatchStr('a', ['A']), ' ', MatchText('a', ['A']), ' ', MatchStr('a', ['b', 'a']));
  WriteLn(AnsiIndexStr('c', ['a', 'b', 'c']), ' ', AnsiIndexText('C', ['a', 'b', 'c']), ' ',
    AnsiMatchStr('z', ['a']), ' ', AnsiMatchText('Z', ['z']));

  WriteLn('[', ReplaceStr('aAa', 'a', 'x'), '] [', ReplaceText('aAa', 'a', 'x'), '] [',
    ReplaceStr('abc', '', 'x'), '] [', ReplaceStr('abcabc', 'bc', ''), '] [',
    AnsiReplaceStr('aa', 'a', 'bb'), '] [', AnsiReplaceText('Aa', 'A', '-'), ']');

  WriteLn('[', LeftStr('hello', 2), '] [', LeftStr('hello', 10), '] [', LeftStr('hello', 0),
    '] [', LeftStr('hello', -1), ']');
  WriteLn('[', RightStr('hello', 2), '] [', RightStr('hello', 10), '] [', RightStr('hello', 0),
    '] [', RightStr('hello', -2), ']');
  WriteLn('[', MidStr('hello', 2, 3), '] [', MidStr('hello', 0, 2), '] [', MidStr('hello', -1, 3),
    '] [', MidStr('hello', 4, 10), '] [', MidStr('hello', 6, 1), '] [', MidStr('hello', 2, -1),
    ']');
  WriteLn('[', AnsiLeftStr('abc', 1), '] [', AnsiRightStr('abc', 1), '] [', AnsiMidStr('abc', 2, 1),
    ']');

  WriteLn('[', DupeString('ab', 3), '] [', DupeString('ab', 1), '] [', DupeString('ab', 0), '] [',
    DupeString('ab', -1), '] [', DupeString('', 5), '] [', Length(DupeString('xyz', 1000)), ']');
  WriteLn('[', ReverseString('abc'), '] [', ReverseString(''), '] [', ReverseString('a'), '] [',
    AnsiReverseString('ab'), ']');
  WriteLn('[', StuffString('abcdef', 2, 3, 'XY'), '] [', StuffString('abc', 4, 0, 'X'), '] [',
    StuffString('abc', 1, 3, ''), ']');

  WriteLn('[', IfThen(True, 'yes', 'no'), '] [', IfThen(False, 'yes', 'no'), '] [',
    IfThen(False, 'yes'), ']');
end.
