program CallFailureValueArgs;

// A method call that fails before it makes the callee's frame (`Self` is nil or freed, or the
// method is abstract) leaves its managed value arguments to the caller, which lends them
// (design/lifetime.md 3.3): nothing leaks, and an instance made in an argument is destroyed when
// the routine ends, like any temporary. Covers CallVirtual, CallVirtualClass, CallIntf and
// CallMethod.

uses System.SysUtils;

type
  TTag = class(TRefObject)
  private
    FName: string;
  public
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  INamed = interface
    procedure SetName(S: string; T: TTag; O: TObject);
  end;

  TShape = class(TRefObject, INamed)
  public
    procedure SetName(S: string; T: TTag; O: TObject); virtual; abstract;
    procedure Plain(S: string; T: TTag; const C: string);
    class procedure Make(S: string; T: TTag); virtual; abstract;
  end;

  TShapeClass = class of TShape;

  TBox = class
  public
    procedure Show(S: string); virtual;
  end;

constructor TTag.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
end;

destructor TTag.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited Destroy;
end;

procedure TShape.Plain(S: string; T: TTag; const C: string);
begin
  WriteLn('plain ', S, C);
end;

procedure TBox.Show(S: string);
begin
  WriteLn('show ', S);
end;

procedure Report(E: Exception);
begin
  WriteLn('caught ', E.ClassName, ': ', E.Message);
end;

var
  X: string;
  C: TShapeClass;
  N: TShape;
  I: INamed;
  B: TBox;
begin
  X := 'ab';
  C := TShape;
  try
    N.SetName(X + 'cd', TTag.Create('nil virtual'), TTag.Create('nil virtual obj'));
  except
    on E: Exception do Report(E);
  end;
  N := C.Create;
  try
    N.SetName(X + 'cd', TTag.Create('abstract virtual'), TTag.Create('abstract virtual obj'));
  except
    on E: Exception do Report(E);
  end;
  try
    C.Make(X + 'cd', TTag.Create('abstract class'));
  except
    on E: Exception do Report(E);
  end;
  I := N;
  try
    I.SetName(X + 'cd', TTag.Create('abstract intf'), TTag.Create('abstract intf obj'));
  except
    on E: Exception do Report(E);
  end;
  I := nil;
  N := nil;
  try
    I.SetName(X + 'cd', TTag.Create('nil intf'), TTag.Create('nil intf obj'));
  except
    on E: Exception do Report(E);
  end;
  C := nil;
  try
    C.Make(X + 'cd', TTag.Create('nil class'));
  except
    on E: Exception do Report(E);
  end;
  try
    N.Plain(X + 'cd', TTag.Create('nil method'), X);
  except
    on E: Exception do Report(E);
  end;
  B := TBox.Create;
  B.Show(X + 'cd');
  B.Free;
  try
    B.Show(X + 'cd');
  except
    on E: Exception do Report(E);
  end;
  WriteLn('end');
end.
