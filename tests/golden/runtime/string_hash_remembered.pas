program StringHashRemembered;

// A string remembers its key hash once worked out (bytecode.md 2.21): the remembered hash is the
// same as one worked out again, and a string whose content is rewritten in place (`S[i] := …`,
// `S := S + …`, `F + …` on a function's result, `A := A + B` on a `UTF8String`) hashes and is
// found by its new content, not its old one.

uses
  System.Generics.Collections;

// A string built at run time whose hash is worked out (and remembered) before it is returned
function Hashed(const Text: string): string;
var
  I: Integer;
begin
  Result := '';
  for I := 1 to Length(Text) do
    Result := Result + Text[I];
  if KeyHash(Result) <> KeyHash(Text) then
    WriteLn('unexpected hash');
end;

var
  D: TDictionary<string, Integer>;
  U: TDictionary<UTF8String, Integer>;
  S, T: string;
  A, B: UTF8String;
  I: Integer;
begin
  D := TDictionary<string, Integer>.Create;
  D.Add('xbc', 1);
  D.Add('abcd', 2);
  // Built at run time, so that the string has a block of its own written in place
  S := '';
  for I := 1 to 3 do
    S := S + Chr(Ord('a') + I - 1);
  T := Copy(S, 1, 3);
  WriteLn('same block twice: ', KeyHash(S) = KeyHash(S));
  WriteLn('same as a fresh string: ', KeyHash(S) = KeyHash(T));
  WriteLn('abc found: ', D.ContainsKey(S));
  S[1] := 'x';
  WriteLn('after S[1] := x: ', S, ' ', D.ContainsKey(S), ' ', KeyHash(S) = KeyHash('xbc'),
    ' ', KeyHash(S) = KeyHash(T));
  S[1] := 'a';
  WriteLn('back to abc: ', D.ContainsKey(S), ' ', KeyHash(S) = KeyHash(T));
  S := S + 'd';
  WriteLn('after S + d: ', S, ' ', D.ContainsKey(S), ' ', KeyHash(S) = KeyHash('abcd'));
  // The result of `Hashed` holds its only reference, so `+ 'e'` appends to it in place
  WriteLn('after Hashed + e: ', KeyHash(Hashed('abc') + 'e') = KeyHash('abce'),
    ' ', D.ContainsKey(Hashed('abc') + 'd'));
  D.Free;

  U := TDictionary<UTF8String, Integer>.Create;
  U.Add(UTF8String('ab'), 1);
  A := '';
  for I := 1 to 2 do
    A := A + UTF8String(Chr(Ord('a') + I - 1));
  B := UTF8String('x');
  WriteLn('UTF8String: ', U.ContainsKey(A), ' ', KeyHash(A) = KeyHash(A));
  A := A + B;
  WriteLn('after A + B: ', U.ContainsKey(A), ' ', KeyHash(A) = KeyHash(UTF8String('abx')));
  U.Add(A, 2);
  WriteLn('abx found: ', U.ContainsKey(UTF8String('abx')));
  U.Free;
end.
