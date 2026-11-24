program StdlibBytesStringStream;

// TBytesStream and TStringStream (spec/stdlib/System.Classes/TStream.md): a TBytesStream made
// from an array shares it until it grows, and Bytes has Size bytes; a TStringStream holds the
// text in its encoding (UTF-8 by default), and is a TMemoryStream but not a TBytesStream.

uses
  System.SysUtils, System.Classes;

procedure ShowBytes(const B: TBytes);
begin
  Write(Length(B), ':');
  for var I := 0 to High(B) do
    Write(' ', B[I]);
  WriteLn;
end;

begin
  var A: TBytes := [1, 2, 3, 4];
  var S := TBytesStream.Create(A);
  WriteLn(S.Size, ' ', S.Position);
  S.Position := 1;
  S.WriteData(Byte(20));
  ShowBytes(A);
  ShowBytes(S.Bytes);
  // Growing copies the bytes to a longer array; A keeps its own
  S.Position := 4;
  S.WriteData(Byte(5));
  S.WriteData(Byte(6));
  ShowBytes(S.Bytes);
  A[0] := 100;
  ShowBytes(S.Bytes);
  S.Size := 2;
  ShowBytes(S.Bytes);
  S.Size := 3;
  ShowBytes(S.Bytes);
  ShowBytes(S.ToBytes);
  S.Clear;
  ShowBytes(S.Bytes);
  // Shrinking a stream that still shares its array cuts a copy: the caller's array keeps its bytes
  var A2: TBytes := [1, 2, 3, 4];
  var S2 := TBytesStream.Create(A2);
  S2.Size := 2;
  ShowBytes(A2);
  ShowBytes(S2.Bytes);
  S2.Size := 4;
  ShowBytes(S2.Bytes);
  var E := TBytesStream.Create;
  E.CopyFrom(TBytesStream.Create([9, 8, 7]), 0);
  ShowBytes(E.Bytes);
  WriteLn(E is TMemoryStream);

  var T := TStringStream.Create('añb');
  WriteLn(T.Size, ' ', T.Position, ' ', T.DataString);
  ShowBytes(T.Bytes);
  T.Position := T.Size;
  T.WriteString('→c');
  WriteLn(T.DataString, ' ', T.Size);
  T.Position := 1;
  WriteLn(T.ReadString(2), ' ', T.Position);
  WriteLn(T.ReadString(100), ' ', T.Position, ' [', T.ReadString(5), ']');
  WriteLn(T.Encoding.EncodingName);

  var U := TStringStream.Create('hi', TEncoding.Unicode, False);
  ShowBytes(U.Bytes);
  WriteLn(U.DataString, ' ', U.Encoding.EncodingName);
  var Raw: TBytes := [$41, $C3, $A9];
  var V := TStringStream.Create(Raw);
  WriteLn(V.DataString, ' ', V.Position);
  var W := TStringStream.Create;
  WriteLn('[', W.DataString, '] ', W.Size);
  WriteLn(T is TMemoryStream, ' ', TObject(T) is TBytesStream);
end.
