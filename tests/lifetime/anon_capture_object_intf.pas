program AnonCaptureObjectIntf;

// Capturing a variable of type `TObject` holding a reference counted instance, and a variable of
// an interface type (objects.md 9.6, 14.9): the cells count what they hold, so the instances live
// while the anonymous method does and are destroyed when it is let go of; nothing leaks.

type
  IGreet = interface
    procedure Greet;
  end;

  TItem = class(TRefObject, IGreet)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
    procedure Greet;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

procedure TItem.Greet;
begin
  WriteLn('greet ', Name);
end;

function Make: TProc;
var
  O: TObject;
  G: IGreet;
begin
  O := TItem.Create('object');
  G := TItem.Create('intf');
  Result := procedure
    begin
      WriteLn('holds ', O.ClassName);
      G.Greet;
    end;
end;

procedure Run;
var
  P: TProc;
begin
  P := Make();
  WriteLn('made');
  P();
  P := nil;
  WriteLn('released');
end;

begin
  Run;
  WriteLn('end');
end.
