program MethodRefBuiltinBody;

// A method reference to a built-in virtual method (objects.md 9.3, 14.9): `ToString` of a
// reference counted class (the class name), `ToString` of an exception (its `Message`) and
// `AfterConstruction` (does nothing) run in the VM without a frame. The instance the call takes
// out and counts is let go of at once, so it is destroyed when its last variable is set to `nil`,
// and nothing leaks.

uses System.SysUtils;

type
  TTextFn = function: string of object;
  TAction = procedure of object;

  TItem = class(TRefObject)
    destructor Destroy; override;
  end;

destructor TItem.Destroy;
begin
  WriteLn('destroy TItem');
  inherited;
end;

procedure Run;
var
  Item: TItem;
  E: Exception;
  Text: TTextFn;
  Act: TAction;
begin
  Item := TItem.Create;
  Text := Item.ToString;
  WriteLn(Text());
  WriteLn(Text());
  Act := Item.AfterConstruction;
  Act;
  WriteLn('set to nil');
  Item := nil;
  WriteLn('assigned: ', Assigned(Text));
  E := Exception.Create('boom');
  Text := E.ToString;
  WriteLn(Text());
  E := nil;
  WriteLn('assigned: ', Assigned(Text));
end;

begin
  Run;
  WriteLn('end');
end.
