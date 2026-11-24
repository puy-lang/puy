program ByteArrayLeak;

// A dynamic array of bytes (held one byte per element; types.md 9) that a leaked instance holds
// is reported by the name of its type, as any other array (diagnostics.md 8.1).

type
  TBytesOf = array of Byte;
  TBuffer = class
    Data: TBytesOf;
  end;

var
  B: TBuffer;
begin
  B := TBuffer.Create;
  SetLength(B.Data, 1000);
  B.Data[999] := 3;
  WriteLn(Length(B.Data) + B.Data[999]);
end.
