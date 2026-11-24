program StdlibMemoryStream;

// TMemoryStream (spec/stdlib/System.Classes/TStream.md): writes and reads across the blocks the
// stream grows by, Position / Size / Seek, seeking past the end (the gap reads as zeros),
// shrinking and growing again (zeros, not the old bytes), Clear, ToBytes, SaveToStream /
// LoadFromStream, CopyFrom, and Close.

uses
  System.SysUtils, System.Classes;

function Pattern(N, Seed: Integer): TBytes;
begin
  SetLength(Result, N);
  for var I := 0 to N - 1 do
    Result[I] := (I * 7 + Seed) mod 251;
end;

function Sum(const B: TBytes): Int64;
begin
  Result := 0;
  for var I := 0 to High(B) do
    Result := Result * 31 mod 1000000007 + B[I];
end;

procedure Show(const What: string; const S: TStream);
begin
  WriteLn(What, ': Size=', S.Size, ' Position=', S.Position);
end;

begin
  var M := TMemoryStream.Create;
  Show('new', M);
  // 1000 bytes in pieces of 300: the blocks are 256, 256, 512 bytes
  var P := Pattern(1000, 3);
  var Off := 0;
  while Off < 1000 do
  begin
    var N := 300;
    if Off + N > 1000 then
      N := 1000 - Off;
    WriteLn('Write ', M.Write(P, Off, N));
    Inc(Off, N);
  end;
  Show('written', M);
  WriteLn('same bytes: ', Sum(M.ToBytes) = Sum(P));

  // A read across the blocks, into the middle of a buffer
  var B: TBytes;
  SetLength(B, 600);
  WriteLn('Seek ', M.Seek(200, TSeekOrigin.soBeginning));
  WriteLn('Read ', M.Read(B, 50, 500));
  var Ok := True;
  for var I := 0 to 499 do
    if B[50 + I] <> P[200 + I] then
      Ok := False;
  WriteLn('read the same: ', Ok, ' ', B[49], ' ', B[550]);
  Show('after read', M);

  // Short read at the end, then nothing
  M.Seek(-10, TSeekOrigin.soEnd);
  WriteLn('Read at end ', M.Read(B, 0, 100), ', again ', M.Read(B, 0, 100));
  WriteLn('Seek soCurrent ', M.Seek(-1000, TSeekOrigin.soCurrent));

  // Past the end: the gap is zeros
  M.Position := 1010;
  M.WriteBuffer(P, 2);
  Show('past the end', M);
  M.Position := 998;
  M.ReadBuffer(B, 14);
  for var I := 0 to 13 do
    Write(B[I], ' ');
  WriteLn;

  // Shrink, then grow: the bytes past the old end are zeros again
  M.Size := 260;
  Show('shrunk', M);
  M.Size := 520;
  M.Position := 258;
  M.ReadBuffer(B, 4);
  WriteLn(B[0], ' ', B[1], ' ', B[2], ' ', B[3]);

  // Save to and load from another stream (the position of the saved one stays)
  var C := TMemoryStream.Create;
  M.Position := 7;
  M.SaveToStream(C);
  Show('saved', M);
  Show('copy', C);
  var D := TMemoryStream.Create;
  D.WriteBuffer(P, 5);
  D.LoadFromStream(C);
  Show('loaded', D);
  WriteLn('loaded the same: ', Sum(D.ToBytes) = Sum(M.ToBytes));

  // CopyFrom: a count from the position, and 0 for all of the source from its start
  var E := TMemoryStream.Create;
  C.Position := 256;
  WriteLn('CopyFrom ', E.CopyFrom(C, 10), ' ', C.Position);
  WriteLn('CopyFrom all ', E.CopyFrom(C, 0), ' ', E.Size);
  try
    C.Position := 515;
    E.CopyFrom(C, 10);
  except
    on X: EReadError do
      WriteLn(X.ClassName, ': ', X.Message);
  end;

  // Errors
  try
    M.Seek(-1, TSeekOrigin.soBeginning);
  except
    on X: EStreamError do
      WriteLn(X.ClassName, ': ', X.Message);
  end;
  try
    M.Size := -1;
  except
    on X: EStreamError do
      WriteLn(X.ClassName, ': ', X.Message);
  end;
  try
    M.Read(B, 590, 20);
  except
    on X: ERangeError do
      WriteLn(X.ClassName, ': ', X.Message);
  end;
  try
    M.Position := 0;
    M.ReadBuffer(B, 600);
  except
    on X: EReadError do
      WriteLn(X.ClassName, ': ', X.Message);
  end;

  M.Clear;
  Show('cleared', M);
  M.Close;
  M.Close;
  try
    M.Write(P, 1);
  except
    on X: EStreamError do
      WriteLn(X.ClassName, ': ', X.Message);
  end;
  WriteLn(M is TStream, ' ', M.CanSeek);
end.
