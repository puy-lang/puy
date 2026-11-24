program TEncodingPuy;

// TEncoding where Puy fixes the behavior (spec/stdlib/System.SysUtils/TEncoding.md): lone
// surrogates (U+FFFD in UTF-8, kept in UTF-16), invalid UTF-8, UTF8Strict, Default (UTF-8
// without a preamble), EncodingName, what ASCII puts for other characters and bytes, an odd
// UTF-16 byte count, the range checks of GetString, GetEncoding and Clone.

uses
  System.SysUtils;

procedure Show(const Title: string; const B: TBytes);
begin
  Write(Title, ' (', Length(B), '):');
  for var X in B do
    Write(' ', IntToHex(X, 2));
  WriteLn;
end;

procedure Codes(const Title, S: string);
begin
  Write(Title, ':');
  for var C in S do
    Write(' ', IntToHex(Ord(C), 4));
  WriteLn;
end;

begin
  var Lone := 'a' + #$D800 + 'b' + #$DE00;
  Show('UTF8 lone', TEncoding.UTF8.GetBytes(Lone));
  Show('Unicode lone', TEncoding.Unicode.GetBytes(Lone));
  Codes('Unicode lone back', TEncoding.Unicode.GetString(TEncoding.Unicode.GetBytes(Lone)));
  Codes('UTF8 invalid', TEncoding.UTF8.GetString([$61, $FF, $C3, $62, $E2, $82]));
  Codes('UTF16 odd', TEncoding.Unicode.GetString([$61, 0, $62]));
  Codes('UTF16BE odd', TEncoding.BigEndianUnicode.GetString([0, $61, $62]));
  Show('ASCII', TEncoding.ASCII.GetBytes('aé😀' + #$D800 + 'z'));
  Codes('ASCII back', TEncoding.ASCII.GetString([$61, $80, $FF, $7F]));

  // UTF8Strict
  Show('strict', TEncoding.UTF8Strict.GetBytes('aé😀'));
  Show('strict preamble', TEncoding.UTF8Strict.GetPreamble);
  try
    TEncoding.UTF8Strict.GetBytes('ab' + #$DC00);
  except
    on E: EEncodingError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    TEncoding.UTF8Strict.GetString([$61, $62, $C3, $28]);
  except
    on E: EEncodingError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    TEncoding.UTF8Strict.GetString([$61, $62, $C3, $A9, $FF], 2, 3);
  except
    on E: EEncodingError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  var Strict := TEncoding.UTF8Strict.Clone;
  try
    Strict.GetBytes(#$D800);
  except
    on E: EEncodingError do WriteLn('clone: ', E.Message);
  end;
  Strict.Free;

  // Default
  Show('Default', TEncoding.Default.GetBytes('é'));
  Show('Default preamble', TEncoding.Default.GetPreamble);
  WriteLn(TEncoding.Default.CodePage, ' ', TEncoding.Default = TEncoding.UTF8, ' ',
    TEncoding.IsStandardEncoding(TEncoding.Default), ' ',
    TEncoding.IsStandardEncoding(TEncoding.UTF8Strict));

  // Names and classes
  var All: TArray<TEncoding> := [TEncoding.UTF8, TEncoding.UTF8Strict, TEncoding.Default,
    TEncoding.Unicode, TEncoding.BigEndianUnicode, TEncoding.ASCII];
  for var E in All do
    WriteLn(E.EncodingName, ' ', E.CodePage, ' ', E.ClassName, ' ', E.IsSingleByte);

  // GetBufferEncoding with a default
  var Found: TEncoding := nil;
  var N := TEncoding.GetBufferEncoding([$61], Found, TEncoding.ASCII);
  WriteLn(N, ' ', Found.EncodingName);
  Found := TEncoding.Default;
  WriteLn(TEncoding.GetBufferEncoding([$EF, $BB, $BF], Found), ' ', Found.EncodingName);

  // The range of GetString
  var Ranges: TArray<TArray<Integer>> := [[0, 3], [3, 0], [-1, 1], [0, -1], [2, 2], [4, 0]];
  for var R in Ranges do
    try
      WriteLn(R[0], ',', R[1], ': [', TEncoding.UTF8.GetString([$61, $62, $63], R[0], R[1]), ']');
    except
      on E: EEncodingError do WriteLn(R[0], ',', R[1], ': ', E.Message);
    end;

  // GetEncoding
  var Pages: TArray<Integer> := [65001, 1200, 1201, 20127, 932];
  for var CP in Pages do
    try
      var Enc := TEncoding.GetEncoding(CP);
      WriteLn(CP, ' ', Enc.ClassName, ' ', TEncoding.IsStandardEncoding(Enc));
      Enc.Free;
    except
      on E: ENotSupportedException do WriteLn(E.ClassName, ': ', E.Message);
    end;
end.
