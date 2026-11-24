unit Settings;

// The use TLazy<T> is meant for (requirements §5.14): `initialization` only creates the TLazy,
// which is light and the same every time, and the work runs at the first read of the value.

interface

uses
  System.SysUtils, Puy.Lazy;

var
  Greeting: TLazy<string>;

implementation

initialization
  WriteLn('initialization of Settings');
  Greeting := TLazy<string>.Create(
    function: string
    begin
      WriteLn('making the greeting');
      Result := 'hello';
    end);
end.
