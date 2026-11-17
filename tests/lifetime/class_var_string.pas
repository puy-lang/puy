program ClassVarString;

// A `class var` lives like a global variable (objects.md 2.2; units.md 5.4): the strings and the
// dynamic array it holds when the program ends are let go when the isolate ends, so nothing is
// reported as a leak. The values assigned before are let go when they are replaced.

uses
  System.SysUtils;

type
  TRegistry = class
    class var Names: string;
    class var Items: array of string;
    class procedure Add(const S: string); static;
  end;
  TChild = class(TRegistry)
  end;

class procedure TRegistry.Add(const S: string);
begin
  Names := Names + S + ';';
  SetLength(Items, Length(Items) + 1);
  Items[High(Items)] := S + '!';
end;

var
  I: Integer;
begin
  for I := 1 to 3 do
    TRegistry.Add('n' + IntToStr(I));
  TChild.Add('child');
  WriteLn(TRegistry.Names, ' ', Length(TChild.Items), ' ', TRegistry.Items[3]);
end.
