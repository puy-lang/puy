program TEncodingCase;

// TEncoding (spec/stdlib/System.SysUtils/TEncoding.md): GetBytes and GetString of UTF8,
// Unicode, BigEndianUnicode and ASCII (ASCII text only: what replaces other characters differs
// by platform), the preambles, the byte order marks found by GetBufferEncoding, Convert, the
// counts and the code pages. Lone surrogates, invalid bytes, Default, UTF8Strict, EncodingName
// and the errors are in golden/runtime/tencoding_puy.

uses
  System.SysUtils;

procedure Show(const Title: string; const B: TBytes);
var
  I: Integer;
begin
  Write(Title, ' (', Length(B), '):');
  for I := 0 to High(B) do
    Write(' ', IntToHex(B[I], 2));
  WriteLn;
end;

function Make(const A: array of Byte): TBytes;
var
  I: Integer;
begin
  SetLength(Result, Length(A));
  for I := 0 to High(A) do
    Result[I] := A[I];
end;

procedure Detect(const Title: string; const B: TBytes);
var
  E: TEncoding;
  N: Integer;
begin
  E := nil;
  N := TEncoding.GetBufferEncoding(B, E);
  Write(Title, ': ', N);
  if E = TEncoding.UTF8 then
    WriteLn(' UTF8')
  else if E = TEncoding.Unicode then
    WriteLn(' Unicode')
  else if E = TEncoding.BigEndianUnicode then
    WriteLn(' BigEndianUnicode')
  else if E = TEncoding.Default then
    WriteLn(' Default')
  else
    WriteLn(' other');
end;

var
  B: TBytes;
  S: string;
  E: TEncoding;
begin
  S := 'aé€' + #$D83D#$DE00;
  B := TEncoding.UTF8.GetBytes(S);
  Show('UTF8', B);
  WriteLn(TEncoding.UTF8.GetString(B) = S);
  WriteLn(TEncoding.UTF8.GetString(B, 1, 5) = 'é€');
  Show('Unicode', TEncoding.Unicode.GetBytes(S));
  WriteLn(TEncoding.Unicode.GetString(TEncoding.Unicode.GetBytes(S)) = S);
  Show('BigEndianUnicode', TEncoding.BigEndianUnicode.GetBytes(S));
  WriteLn(TEncoding.BigEndianUnicode.GetString(TEncoding.BigEndianUnicode.GetBytes(S)) = S);
  Show('ASCII', TEncoding.ASCII.GetBytes('Hi!'));
  WriteLn(TEncoding.ASCII.GetString(TEncoding.ASCII.GetBytes('Hi!')));
  Show('empty', TEncoding.UTF8.GetBytes(''));
  WriteLn('[', TEncoding.UTF8.GetString(nil), ']');

  Show('UTF8 preamble', TEncoding.UTF8.GetPreamble);
  Show('Unicode preamble', TEncoding.Unicode.GetPreamble);
  Show('BigEndianUnicode preamble', TEncoding.BigEndianUnicode.GetPreamble);
  Show('ASCII preamble', TEncoding.ASCII.GetPreamble);

  Detect('utf-8 bom', Make([$EF, $BB, $BF, $61]));
  Detect('utf-16le bom', Make([$FF, $FE, $61, 0]));
  Detect('utf-16be bom', Make([$FE, $FF, 0, $61]));
  Detect('none', Make([$61, $62]));
  Detect('short', Make([$EF, $BB]));
  E := TEncoding.Unicode;
  WriteLn('given Unicode: ', TEncoding.GetBufferEncoding(Make([$FF, $FE, $61, 0]), E));
  E := TEncoding.Unicode;
  WriteLn('given Unicode, utf-8 bom: ',
    TEncoding.GetBufferEncoding(Make([$EF, $BB, $BF]), E));

  Show('Convert', TEncoding.Convert(TEncoding.UTF8, TEncoding.BigEndianUnicode,
    TEncoding.UTF8.GetBytes('aé')));
  Show('Convert range', TEncoding.Convert(TEncoding.Unicode, TEncoding.UTF8,
    TEncoding.Unicode.GetBytes('xé€y'), 2, 4));
  WriteLn(TEncoding.UTF8.GetByteCount(S), ' ', TEncoding.Unicode.GetByteCount(S));
  WriteLn(TEncoding.UTF8.GetCharCount(B), ' ', TEncoding.Unicode.GetCharCount(
    TEncoding.Unicode.GetBytes(S)));
  WriteLn(TEncoding.UTF8.CodePage, ' ', TEncoding.Unicode.CodePage, ' ',
    TEncoding.BigEndianUnicode.CodePage, ' ', TEncoding.ASCII.CodePage);
  WriteLn(TEncoding.UTF8.IsSingleByte, ' ', TEncoding.ASCII.IsSingleByte);
  WriteLn(TEncoding.IsStandardEncoding(TEncoding.UTF8), ' ',
    TEncoding.IsStandardEncoding(nil));
  E := TEncoding.UTF8.Clone;
  try
    WriteLn(TEncoding.IsStandardEncoding(E), ' ', E.CodePage, ' ', E.GetString(B) = S);
  finally
    E.Free;
  end;
end.
