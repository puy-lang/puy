program StdlibCustomStream;

// A stream written by the program on TStream's three cores (spec/stdlib/System.Classes/
// TStream.md): Size, Position, ReadBuffer, ReadData and CopyFrom work through them; a core that
// gives fewer bytes than asked is read again; SetSize of TStream raises EStreamError.

uses
  System.SysUtils, System.Classes;

type
  // The bytes 0, 1, 2, … (mod 256), Len of them, given at most 3 at a time
  TCounting = class(TStream)
  private
    FLen: Int64;
    FPos: Int64;
  public
    constructor Create(Len: Int64);
    function Read(var Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Write(const Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Seek(const Offset: Int64; Origin: TSeekOrigin): Int64; override;
  end;

constructor TCounting.Create(Len: Int64);
begin
  inherited Create;
  FLen := Len;
end;

function TCounting.Read(var Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  Result := 0;
  while (Result < Count) and (Result < 3) and (FPos < FLen) do
  begin
    Buffer[Offset + Result] := FPos mod 256;
    Inc(Result);
    Inc(FPos);
  end;
end;

function TCounting.Write(const Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  Result := 0;
end;

function TCounting.Seek(const Offset: Int64; Origin: TSeekOrigin): Int64;
begin
  case Origin of
    TSeekOrigin.soBeginning:
      FPos := Offset;
    TSeekOrigin.soCurrent:
      FPos := FPos + Offset;
  else
    FPos := FLen + Offset;
  end;
  Result := FPos;
end;

begin
  var C := TCounting.Create(100000);
  WriteLn(C.Size, ' ', C.Position);
  C.Position := 254;
  var W: Word := 0;
  WriteLn(C.ReadData(W), ' ', W);
  var B: TBytes;
  SetLength(B, 10);
  C.ReadBuffer(B, 10);
  WriteLn(B[0], ' ', B[9], ' ', C.Position);
  var M := TMemoryStream.Create;
  WriteLn(M.CopyFrom(C, 0), ' ', M.Size, ' ', C.Position);
  var Ok := True;
  var All := M.ToBytes;
  for var I := 0 to High(All) do
    if All[I] <> I mod 256 then
      Ok := False;
  WriteLn(Ok);
  C.Position := 99995;
  try
    M.CopyFrom(C, 10);
  except
    on E: EReadError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    C.Size := 5;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    C.WriteBuffer(B, 1);
  except
    on E: EWriteError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    C.CopyFrom(C, -1);
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
