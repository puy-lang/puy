// puy-test: test-fs
program IOUtilsFiles;

// TFile on the runner's file system in memory (spec/stdlib/System.IOUtils/TFile.md): the
// encodings and BOMs of the text functions, lines split at CR LF / LF / CR, ReadLines across
// the reads of 64KB and in UTF-16, the modes of Open, the size, the attributes and the times in
// UTC (the memory host gives every file 2026-01-02T03:04:05Z until they are set).

uses
  System.SysUtils, System.Classes, System.IOUtils;

procedure ShowBytes(const What: string; const B: TBytes);
begin
  Write(What, ':');
  for var X in B do
    Write(' ', IntToHex(X, 2));
  WriteLn;
end;

procedure ShowLines(const What: string; const L: TArray<string>);
begin
  Write(What, ': ', Length(L));
  for var S in L do
    Write(' [', S, ']');
  WriteLn;
end;

// Seconds since the UNIX epoch of a TDateTime in UTC
function UnixSeconds(const T: TDateTime): Int64;
begin
  Result := Round((T - 25569) * 86400);
end;

begin
  TFile.WriteAllText('t.txt', 'é');
  ShowBytes('WriteAllText', TFile.ReadAllBytes('t.txt'));
  TFile.WriteAllText('t.txt', 'é', TEncoding.UTF8);
  ShowBytes('WriteAllText UTF8', TFile.ReadAllBytes('t.txt'));
  WriteLn('ReadAllText (BOM skipped): ', TFile.ReadAllText('t.txt'), ' ',
    Length(TFile.ReadAllText('t.txt')));
  TFile.WriteAllText('t.txt', 'aé', TEncoding.Unicode);
  ShowBytes('WriteAllText Unicode', TFile.ReadAllBytes('t.txt'));
  WriteLn('ReadAllText (UTF-16 BOM): ', TFile.ReadAllText('t.txt'));
  TFile.WriteAllText('t.txt', 'aé', TEncoding.BigEndianUnicode);
  WriteLn('ReadAllText (UTF-16BE BOM): ', TFile.ReadAllText('t.txt'));
  TFile.WriteAllBytes('t.txt', TEncoding.Unicode.GetBytes('hi'));
  WriteLn('ReadAllText Unicode without BOM: ', TFile.ReadAllText('t.txt', TEncoding.Unicode));
  TFile.WriteAllText('t.txt', 'x', TEncoding.ASCII);
  ShowBytes('WriteAllText ASCII (no preamble)', TFile.ReadAllBytes('t.txt'));
  TFile.AppendAllText('t.txt', 'yé', TEncoding.Unicode);
  ShowBytes('AppendAllText Unicode (no BOM)', TFile.ReadAllBytes('t.txt'));

  TFile.WriteAllText('l.txt', 'a'#13#10'b'#10#10'c'#13);
  ShowLines('ReadAllLines', TFile.ReadAllLines('l.txt'));
  TFile.WriteAllText('l.txt', '');
  ShowLines('ReadAllLines empty', TFile.ReadAllLines('l.txt'));
  TFile.WriteAllText('l.txt', 'no end');
  ShowLines('ReadAllLines no end', TFile.ReadAllLines('l.txt'));
  TFile.WriteAllLines('l.txt', ['x', '', 'y']);
  ShowBytes('WriteAllLines', TFile.ReadAllBytes('l.txt'));
  TFile.WriteAllLines('l.txt', ['é'], TEncoding.Unicode);
  ShowLines('ReadAllLines Unicode', TFile.ReadAllLines('l.txt', TEncoding.Unicode));

  // ReadLines: a CR at the end of the first 64KB read and its LF in the next
  var Big := StringOfChar('a', 65535) + #13#10'second'#13'third'#10;
  TFile.WriteAllText('big.txt', Big);
  var N := 0;
  for var Line in TFile.ReadLines('big.txt') do
  begin
    Inc(N);
    WriteLn('ReadLines ', N, ': ', Length(Line), ' ', Copy(Line, 1, 6));
  end;
  TFile.WriteAllText('u.txt', 'one'#13#10'twoé'#10'three', TEncoding.Unicode);
  var Lines := TFile.ReadLines('u.txt');
  for var Line in Lines do
    Write('[', Line, ']');
  WriteLn;
  // Each enumeration reads the file again
  for var Line in Lines do
    Write('<', Line, '>');
  WriteLn;
  TFile.WriteAllText('u.txt', 'be'#10'x', TEncoding.BigEndianUnicode);
  for var Line in TFile.ReadLines('u.txt') do
    Write('[', Line, ']');
  WriteLn;
  TFile.WriteAllText('u.txt', '');
  for var Line in TFile.ReadLines('u.txt') do
    Write('[', Line, ']');
  WriteLn('(empty)');

  // The modes of Open
  var S := TFile.Create('o.bin');
  S.WriteData(Integer(1));
  S.Close;
  S := TFile.Open('o.bin', TFileMode.fmAppend);
  S.WriteData(Integer(2));
  WriteLn('fmAppend position: ', S.Position);
  S.Close;
  S := TFile.Open('o.bin', TFileMode.fmOpen, TFileAccess.faRead);
  WriteLn('fmOpen faRead size: ', S.Size);
  try
    S.WriteData(Integer(3));
  except
    on E: EWriteError do
      WriteLn('write to faRead: ', E.ClassName);
  end;
  S.Close;
  S := TFile.Open('o.bin', TFileMode.fmOpenOrCreate);
  WriteLn('fmOpenOrCreate existing size: ', S.Size);
  S.Close;
  S := TFile.Open('new.bin', TFileMode.fmOpenOrCreate);
  WriteLn('fmOpenOrCreate new size: ', S.Size, ' ', TFile.Exists('new.bin'));
  S.Close;
  S := TFile.Open('o.bin', TFileMode.fmTruncate);
  WriteLn('fmTruncate size: ', S.Size);
  S.Close;
  S := TFile.Open('cn.bin', TFileMode.fmCreateNew);
  WriteLn('fmCreateNew: ', TFile.Exists('cn.bin'));
  S.Close;
  S := TFile.OpenWrite('ow.bin');
  S.WriteData(Byte(7));
  S.Close;
  S := TFile.OpenRead('ow.bin');
  WriteLn('OpenWrite / OpenRead: ', S.Size);
  S.Close;
  WriteLn('GetSize: ', TFile.GetSize('big.txt'));

  // Attributes
  WriteLn('attributes: ', TFile.GetAttributes('o.bin') = [TFileAttribute.faNormal], ' ',
    TFile.GetAttributes('.') = [TFileAttribute.faDirectory]);
  TFile.SetAttributes('o.bin', [TFileAttribute.faReadOnly]);
  WriteLn('read-only: ', TFile.GetAttributes('o.bin') = [TFileAttribute.faReadOnly]);
  try
    TFile.WriteAllText('o.bin', 'x');
  except
    on E: EFCreateError do
      WriteLn('write read-only: ', E.ClassName, ' ', E.ErrorKind);
  end;
  TFile.SetAttributes('o.bin', []);
  WriteLn('not read-only: ', TFile.GetAttributes('o.bin') = [TFileAttribute.faNormal]);

  // Times (UTC)
  WriteLn('last write: ', UnixSeconds(TFile.GetLastWriteTimeUtc('o.bin')));
  TFile.SetLastWriteTimeUtc('o.bin', 25569 + 1.5);
  TFile.SetLastAccessTimeUtc('o.bin', 25569 - 0.25);
  TFile.SetCreationTimeUtc('o.bin', 25569 + 20000);
  WriteLn('last write: ', UnixSeconds(TFile.GetLastWriteTimeUtc('o.bin')));
  WriteLn('last access: ', UnixSeconds(TFile.GetLastAccessTimeUtc('o.bin')));
  WriteLn('creation: ', UnixSeconds(TFile.GetCreationTimeUtc('o.bin')));
  TFile.Move('o.bin', 'moved.bin');
  WriteLn('moved keeps the time: ', UnixSeconds(TFile.GetLastWriteTimeUtc('moved.bin')));
  TFile.Copy('moved.bin', 'copied.bin');
  TFile.Copy('t.txt', 'copied.bin', True);
  WriteLn('Copy overwrite: ', TFile.GetSize('copied.bin') = TFile.GetSize('t.txt'));
  TFile.Delete('copied.bin');
  WriteLn('Delete: ', TFile.Exists('copied.bin'));
end.
