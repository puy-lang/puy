unit Reflected;

interface

implementation

uses System.Rtti;

type
  TMarkerAttribute = class(TCustomAttribute)
  end;

  [TMarker]
  TBox<T> = class
  private
    function GetValue: Integer;
    procedure SetValue(N: Integer);
  public
    function Add(N: Integer): Integer;
    property Value: Integer read GetValue write SetValue;
  end;

var
  Stored: Integer;

function TBox<T>.GetValue: Integer;
begin
  Result := Stored;
end;

procedure TBox<T>.SetValue(N: Integer);
begin
  Stored := N;
end;

function TBox<T>.Add(N: Integer): Integer;
begin
  Result := Stored + N;
end;

procedure Test;
begin
  // Both instantiations use identical code, so codegen shares their methods.
  var A := TBox<Integer>.Create;
  var B := TBox<Boolean>.Create;
  try
    var Ctx := TRttiContext.Create;
    var Info := Ctx.GetType(TypeInfo(TBox<Boolean>));
    Info.GetProperty('Value').SetValue(B, 40);
    WriteLn(Info.GetProperty('Value').GetValue(B).AsInteger);
    WriteLn(Info.GetMethod('Add').Invoke(B, [2]).AsInteger);
    WriteLn(Ctx.GetType(TypeInfo(TBox<Integer>)).GetMethod('Add').Invoke(A, [3]).AsInteger);
    WriteLn(Info.GetAttributes[0].ClassName);
  finally
    B.Free;
    A.Free;
  end;
end;

initialization
  Test;
end.
