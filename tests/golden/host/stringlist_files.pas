// puy-test: test-fs
program StringListFiles;

// TStringList's files and streams (docs/spec/stdlib/System.Classes/TStringList.md 2.6): the
// bytes SaveToStream writes with and without an encoding, LoadFromStream finding the encoding
// by its BOM and remembering it for the next save, reading from the position of the stream,
// and SaveToFile / LoadFromFile.

uses
  System.SysUtils, System.Classes;

procedure ShowBytes(const What: string; const B: TBytes);
var
  I: Integer;
begin
  Write(What, ':');
  for I := 0 to High(B) do
    Write(' ', IntToHex(B[I], 2));
  WriteLn;
end;

procedure Show(const What: string; L: TStrings);
var
  I: Integer;
begin
  Write(What, ' (', L.Count, '):');
  for I := 0 to L.Count - 1 do
    Write(' [', L[I], ']');
  WriteLn;
end;

function Bytes(const B: TBytes): TBytesStream;
begin
  Result := TBytesStream.Create(B);
end;

function Saved(L: TStrings; Encoding: TEncoding): TBytes;
begin
  var S := TBytesStream.Create;
  L.SaveToStream(S, Encoding);
  Result := S.Bytes;
end;

begin
  var L := TStringList.Create;
  L.Add('a');
  L.Add('é');
  ShowBytes('default', Saved(L, nil));
  ShowBytes('UTF8', Saved(L, TEncoding.UTF8));
  ShowBytes('Unicode', Saved(L, TEncoding.Unicode));
  ShowBytes('BigEndianUnicode', Saved(L, TEncoding.BigEndianUnicode));
  L.LineBreak := #13#10;
  ShowBytes('CR LF', Saved(L, nil));

  var M := TStringList.Create;
  M.LoadFromStream(TBytesStream.Create(Saved(L, TEncoding.Unicode)));
  Show('UTF-16LE', M);
  WriteLn(M.Encoding.EncodingName);
  ShowBytes('saved again', Saved(M, nil));
  M.LoadFromStream(Bytes([$EF, $BB, $BF, $78, $0A, $79]));
  Show('UTF-8 BOM', M);
  WriteLn(M.Encoding = TEncoding.UTF8);
  M.LoadFromStream(Bytes([$FF, $FE, $78]), TEncoding.UTF8);
  Show('given UTF8', M);
  M.LoadFromStream(Bytes([$78, $C3]));
  Show('invalid', M);
  WriteLn(M.Encoding = TEncoding.Default);
  var S := Bytes([$61, $0A, $62, $0A, $63]);
  S.Position := 2;
  M.LoadFromStream(S);
  Show('from the position', M);
  M.LoadFromStream(TBytesStream.Create(nil));
  Show('empty', M);

  var E := TEncoding.GetEncoding(1201);
  L.SaveToFile('lines.txt', E);
  M.LoadFromFile('lines.txt');
  Show('file', M);
  WriteLn(M.Encoding.EncodingName, ' ', M.Encoding = TEncoding.BigEndianUnicode);
  // An encoding that is not one of the standard ones is copied: the list keeps its copy
  M.LoadFromFile('lines.txt', E);
  E.Free;
  WriteLn(M.Encoding.EncodingName, ' ', TEncoding.IsStandardEncoding(M.Encoding));
  // Reading again with the list's own copy keeps it usable
  M.LoadFromFile('lines.txt', M.Encoding);
  WriteLn(M.Encoding.EncodingName, ' ', TEncoding.IsStandardEncoding(M.Encoding));
  ShowBytes('own copy', Saved(M, M.Encoding));
  M.LoadFromFile('lines.txt', TEncoding.BigEndianUnicode);
  Show('file given', M);
  M.Add('c');
  M.SaveToFile('lines.txt');
  var F := TFileStream.Create('lines.txt', fmOpenRead);
  WriteLn(F.Size);
  F.Close;
  try
    M.LoadFromFile('missing.txt');
  except
    on X: EFOpenError do
      WriteLn(X.ClassName, ' ', X.ErrorKind = TIOErrorKind.ekNotFound);
  end;
  Show('kept', M);
  L.Free;
  M.Free;
end.
