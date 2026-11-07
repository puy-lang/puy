program UnicodeStrings;

// Non-ASCII strings (design/testing.md 3.5; within the range where FPC's delphiunicode means the same)

uses
  System.SysUtils;

var
  S, T: string;
  I: Integer;
  N: Integer;
begin
  S := 'こんにちは、世界';
  WriteLn(S);
  WriteLn(Length(S));
  WriteLn(S[1], S[Length(S)]);
  WriteLn(Copy(S, 7, 2));
  WriteLn(Pos('世界', S), Pos('x', S));
  T := S + '!';
  WriteLn(T);
  WriteLn(S = 'こんにちは、世界', S <> T);
  WriteLn('あ' < 'い', 'い' < 'あ', 'é' < 'z', 'Z' < 'a');
  WriteLn(UpperCase('straße é'), LowerCase('ÀÉ ABC'));
  WriteLn(Ord('é'), ' ', Ord('あ'));
  WriteLn(#$3042#$3044, #12354);
  WriteLn(Trim(#$A0'x'#$A0) = 'x', Trim(' x ') = 'x');
  N := 0;
  for I := 1 to Length(S) do
    if S[I] = 'ん' then
      Inc(N);
  WriteLn(N);
  T := S;
  T[1] := 'コ';
  WriteLn(S, ' ', T);
  WriteLn(IntToStr(Length('日本語')) + '文字');
end.
