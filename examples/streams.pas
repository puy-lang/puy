program Streams;

uses
  System.SysUtils, System.Classes, Puy.Console;

// Read name=value settings from UTF-8 and write the chosen values to a binary stream.
procedure RoundTrip;
begin
  var Text := TMemoryStream.Create;
  var TextWriter := TStreamWriter.Create(Text, TEncoding.UTF8);
  TextWriter.NewLine := #10;
  TextWriter.WriteLine('name=café');
  TextWriter.WriteLine('copies=2');
  TextWriter.Flush;

  Text.Position := 0;
  var TextReader := TStreamReader.Create(Text, TEncoding.UTF8, True);
  var Settings := TStringList.Create;
  try
    while not TextReader.EndOfStream do
      Settings.Add(TextReader.ReadLine);
    var Copies := StrToInt(Settings.Values['copies']);

    var Data := TMemoryStream.Create;
    var Writer := TBinaryWriter.Create(Data, TEncoding.UTF8);
    Writer.Write(Copies);
    Writer.Write(Settings.Values['name']);

    // Reopen the payload from its bytes, starting at position zero.
    var Payload := TBytesStream.Create(Data.ToBytes);
    var Reader := TBinaryReader.Create(Payload, TEncoding.UTF8);
    var OutputCopies := Reader.ReadInteger;
    var OutputName := Reader.ReadString;
    var NameBytes := TEncoding.UTF8.GetBytes(OutputName);
    Write('Binary round trip: ');
    TConsole.Output.WriteBuffer(NameBytes, 0, Length(NameBytes));
    WriteLn(' x', OutputCopies, ' (', Length(NameBytes), ' UTF-8 bytes)');
  finally
    Settings.Free;
  end;
end;

begin
  RoundTrip;
end.
