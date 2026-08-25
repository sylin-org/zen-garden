import { Outlet } from "react-router-dom";
import Breadcrumb from "../../components/Breadcrumb";

export default function ManageSurface() {
  return (
    <div className="flex flex-col h-full">
      <Breadcrumb />
      <div className="flex-1 overflow-hidden">
        <Outlet />
      </div>
    </div>
  );
}
